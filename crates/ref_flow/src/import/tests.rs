use super::*;
use flate2::Compression;
use flate2::write::GzEncoder;
use std::io::Write;

fn tmp_dir() -> PathBuf {
    // Tests run in parallel; a clock-based name alone collided.
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "ref_flow_import_{}_{}_{seq}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_gz(dir: &Path, name: &str, lines: &[Value], truncate_tail: bool) -> PathBuf {
    let path = dir.join(name);
    let mut enc = GzEncoder::new(Vec::new(), Compression::fast());
    for l in lines {
        enc.write_all(serde_json::to_string(l).unwrap().as_bytes())
            .unwrap();
        enc.write_all(b"\n").unwrap();
    }
    let mut bytes = enc.finish().unwrap();
    if truncate_tail {
        bytes.truncate(bytes.len() * 2 / 3);
    }
    std::fs::write(&path, bytes).unwrap();
    path
}

fn fixture(name: &str) -> Value {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enrich")
            .join(format!("{name}.json")),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn list_items() -> Vec<Value> {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/list_items.json"
    )))
    .unwrap()
}

const WIDE: DateRange = DateRange {
    since: NaiveDate::MIN,
    until: NaiveDate::MAX,
};

#[test]
fn classifies_archive_files_by_tag() {
    let k = |n: &str| classify(Path::new(n));
    assert_eq!(k("ref_20261025T060047Z_fwd.jsonl.gz"), Some(RawKind::List));
    assert_eq!(k("ref_20261001T000000Z_gap.jsonl.gz"), Some(RawKind::List));
    assert_eq!(k("ref_20261001T000000Z_bf.jsonl.gz"), Some(RawKind::List));
    assert_eq!(
        k("ref_20260818T124053Z_fwd_swap.jsonl.gz"),
        None,
        "v4 per-method files are not supported"
    );
    assert_eq!(
        k("ref_20261025T060144Z_enrich.jsonl.gz"),
        Some(RawKind::Enrich)
    );
    assert_eq!(k("notes.txt"), None);
}

#[test]
fn reads_complete_and_truncated_gzip_files() {
    let dir = tmp_dir();
    let items = list_items();
    let ok = write_gz(&dir, "a_fwd.jsonl.gz", &items, false);
    let (lines, truncated) = read_lines(&ok).unwrap();
    assert_eq!(lines.len(), items.len());
    assert!(!truncated);

    let cut = write_gz(&dir, "b_fwd.jsonl.gz", &items, true);
    let (lines, truncated) = read_lines(&cut).unwrap();
    assert!(truncated);
    assert!(
        lines.len() < items.len(),
        "partial file yields the complete prefix"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn loads_receipts_with_dedup_and_date_filter() {
    let dir = tmp_dir();
    let items = list_items();
    let a = write_gz(&dir, "a_fwd.jsonl.gz", &items, false);
    let b = write_gz(&dir, "b_gap.jsonl.gz", &items[..2], false); // duplicates
    let (receipts, summary) = load_receipts(&[a.clone(), b.clone()], WIDE).unwrap();
    // 4 Ref swap/ft receipts in the fixture; the other two items are skipped.
    assert_eq!(receipts.len(), 4);
    assert_eq!(summary.receipts, 4);
    assert_eq!(
        summary.ok_receipts, 2,
        "swap_ok and ft_ok; swap_withdraw in the fixture failed"
    );
    assert_eq!(summary.skipped_other, 2);
    assert_eq!(summary.lines, 8);
    assert_eq!(summary.parse_errors, 0);

    let narrow = DateRange {
        since: NaiveDate::from_ymd_opt(2030, 1, 1).unwrap(),
        until: NaiveDate::MAX,
    };
    let (none, summary) = load_receipts(&[a, b], narrow).unwrap();
    assert!(none.is_empty());
    assert_eq!(summary.out_of_range, 6);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn groups_enrich_lines_by_tx_and_parses_trades() {
    let dir = tmp_dir();
    let mut untagged = fixture("swap_chain");
    untagged.as_object_mut().unwrap().remove("_tx_hash");
    let lines = vec![
        fixture("swap_single"),
        fixture("ft_basic"),
        fixture("ft_nested_client_echo"),
        untagged,
    ];
    let path = write_gz(&dir, "x_enrich.jsonl.gz", &lines, false);
    let (txs, mut summary) = load_enriched(&[path]).unwrap();
    assert_eq!(summary.lines, 4);
    assert_eq!(summary.lines_without_tx, 1);
    assert_eq!(txs.len(), 3);

    let parsed = parse_enriched(&txs, WIDE, &mut summary);
    assert_eq!(parsed.len(), 3);
    assert_eq!(summary.trades, 3);
    assert_eq!(summary.rejects, 0);
    assert_eq!(summary.stats.accepted, 3);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn comparison_classifies_matches_new_and_mismatches() {
    let txs = vec![
        EnrichedTx {
            tx_hash: TxHash::new(fixture("swap_single")["_tx_hash"].as_str().unwrap()).unwrap(),
            roots: vec![fixture("swap_single")],
        },
        EnrichedTx {
            tx_hash: TxHash::new(fixture("ft_near_withdraw")["_tx_hash"].as_str().unwrap())
                .unwrap(),
            roots: vec![fixture("ft_near_withdraw")],
        },
    ];
    let mut summary = EnrichSummary::default();
    let parsed = parse_enriched(&txs, WIDE, &mut summary);
    let trades: Vec<&Trade> = parsed.iter().flat_map(|(_, p)| p.trades.iter()).collect();
    assert_eq!(trades.len(), 2);
    let single = trades
        .iter()
        .find(|t| t.route == dex::TradeRoute::Swap)
        .unwrap();

    let mut csv = HashMap::new();
    csv.insert(
        single.receipt_id.as_str().to_owned(),
        CsvTrade {
            token_in: "zec.omft.near".to_owned(),
            amount_in: "2991757".to_owned(),
            token_out: "wrap.near".to_owned(),
            amount_out: "7682134962619246593392108".to_owned(),
        },
    );
    csv.insert(
        "csvonly".to_owned(),
        CsvTrade {
            token_in: "a".into(),
            amount_in: "1".into(),
            token_out: "b".into(),
            amount_out: "1".into(),
        },
    );
    let cmp = compare(&trades, &csv);
    assert_eq!(cmp.matched, 1);
    assert_eq!(
        cmp.new_here, 1,
        "near_withdraw trade is new relative to the CSV"
    );
    assert!(cmp.mismatched.is_empty());
    assert_eq!(cmp.csv_only, 1);

    csv.get_mut(single.receipt_id.as_str()).unwrap().amount_out = "1".to_owned();
    let cmp = compare(&trades, &csv);
    assert_eq!(cmp.matched, 0);
    assert_eq!(cmp.mismatched, vec![single.receipt_id.as_str().to_owned()]);
}

#[test]
fn trades_csv_loader_reads_the_python_columns() {
    let dir = tmp_dir();
    let path = dir.join("trades.csv");
    std::fs::write(
        &path,
        "ts_ns,utc,block_height,tx_hash,receipt_id,sender,stratum,route,hops,pools,token_in,amount_in,token_out,amount_out\n\
         1,2026-01-01 00:00:00,1,tx,rid1,s,organic,swap,1,6065,zec.omft.near,5,wrap.near,7\n\
         short,line\n",
    )
    .unwrap();
    let csv = load_trades_csv(&path).unwrap();
    assert_eq!(csv.len(), 1);
    assert_eq!(csv["rid1"].amount_out, "7");
    std::fs::remove_dir_all(dir).unwrap();
}
