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

// ------------------------------------------------------------ apply (DB)

mod apply {
    use super::*;
    use chrono::{NaiveDateTime, TimeDelta, Timelike, Utc};
    use common::types::{ReceiptId, TokenAccount, TokenSmallestUnits};
    use dex::ref_trade::TradeParts;
    use dex::{Hop, TradeRoute};
    use persistence::ref_flow::receipt::{RefReceipt, RefReceiptParts, ingest_receipts};
    use persistence::ref_flow::test_support::{
        FEE_FIXTURE_POOL_ID_MIN, clean_tables, clear_pool_fee_snapshots, insert_pool_fee_snapshot,
    };
    use persistence::ref_flow::trade as trade_store;
    use persistence::ref_flow::tx as tx_store;
    use serial_test::serial;

    const POOL_A: u32 = FEE_FIXTURE_POOL_ID_MIN + 11;
    const POOL_B: u32 = FEE_FIXTURE_POOL_ID_MIN + 12;
    const POOL_UNKNOWN: u32 = FEE_FIXTURE_POOL_ID_MIN + 13;

    fn id(n: u32) -> String {
        let mut bytes = [0u8; 32];
        bytes[..4].copy_from_slice(&n.to_be_bytes());
        bytes[31] = 7;
        bs58::encode(bytes).into_string()
    }

    fn tx_hash(n: u32) -> TxHash {
        TxHash::new(&id(n)).unwrap()
    }

    fn hop(pool_id: u32) -> Hop {
        let a: TokenAccount = "a.near".parse().unwrap();
        let b: TokenAccount = "b.near".parse().unwrap();
        Hop {
            pool_id,
            token_in: a.to_in(),
            token_out: b.to_out(),
            amount_in: TokenSmallestUnits::from_u128(1_000),
            amount_out: TokenSmallestUnits::from_u128(999),
        }
    }

    fn trade(n: u32, tx: u32, at: NaiveDateTime, hops: Vec<Hop>) -> Trade {
        Trade::new(TradeParts {
            receipt_id: ReceiptId::new(&id(500_000 + n)).unwrap(),
            tx_hash: tx_hash(tx),
            block_height: 1,
            block_time: at,
            sender: "trader.near".parse().unwrap(),
            route: TradeRoute::Swap,
            amount_in: hops[0].amount_in.clone(),
            amount_out: hops[hops.len() - 1].amount_out.clone(),
            hops,
            fees: None,
        })
        .unwrap()
    }

    /// Frame row for `tx` at `at` (one successful swap receipt).
    async fn seed(tx: u32, at: NaiveDateTime) -> Result<()> {
        let receipt = RefReceipt::new(RefReceiptParts {
            receipt_id: ReceiptId::new(&id(700_000 + tx)).unwrap(),
            tx_hash: tx_hash(tx),
            block_height: 1,
            block_time: at,
            predecessor: "seed.near".parse().unwrap(),
            methods: vec!["swap".to_owned()],
            ok: true,
        })
        .unwrap();
        ingest_receipts(&[receipt]).await?;
        Ok(())
    }

    fn parsed(trades: Vec<Trade>) -> ParsedTx {
        ParsedTx {
            trades,
            ..ParsedTx::default()
        }
    }

    async fn stored_fees(tx: u32) -> Result<Vec<Option<Vec<u32>>>> {
        Ok(trade_store::get_by_tx_hash(&tx_hash(tx))
            .await?
            .iter()
            .map(|t| t.fees().map(<[u32]>::to_vec))
            .collect())
    }

    #[tokio::test]
    #[serial(ref_flow_db, pool_info)]
    async fn fees_follow_each_trade_time_all_or_nothing() -> Result<()> {
        clean_tables().await?;
        clear_pool_fee_snapshots().await?;
        let now = Utc::now().naive_utc();
        let h = |n: i64| now - TimeDelta::hours(n);
        insert_pool_fee_snapshot(POOL_A, 30, h(3)).await?;
        insert_pool_fee_snapshot(POOL_A, 40, h(1)).await?;
        insert_pool_fee_snapshot(POOL_B, 7, h(3)).await?;
        for (tx, at) in [
            (1, h(2)),
            (2, h(4)),
            (3, h(1)),
            (4, h(2)),
            (5, h(2)),
            (6, h(2)),
        ] {
            seed(tx, at).await?;
        }
        let ns = h(2).with_nanosecond(123_456_789).unwrap();
        // Hash order on purpose: the chunk mixes every instant.
        let input = vec![
            (
                tx_hash(1),
                parsed(vec![trade(1, 1, h(2), vec![hop(POOL_A)])]),
            ), // between snapshots
            (
                tx_hash(2),
                parsed(vec![trade(2, 2, h(4), vec![hop(POOL_A)])]),
            ), // before the first one
            (
                tx_hash(3),
                parsed(vec![trade(3, 3, h(1), vec![hop(POOL_A), hop(POOL_B)])]),
            ), // 2 hops
            (
                tx_hash(4),
                parsed(vec![trade(
                    4,
                    4,
                    h(2),
                    vec![hop(POOL_A), hop(POOL_UNKNOWN)],
                )]),
            ), // one hop unknown
            (tx_hash(5), parsed(vec![trade(5, 5, ns, vec![hop(POOL_A)])])), // nanoseconds
            (tx_hash(6), parsed(vec![])),                                   // no trades
        ];
        let out = apply_enriched(&input, now).await?;
        assert_eq!(out.recorded, 6);
        assert_eq!(out.trades, 5);
        assert_eq!(
            stored_fees(1).await?,
            vec![Some(vec![30])],
            "older snapshot applies"
        );
        assert_eq!(stored_fees(2).await?, vec![None], "no snapshot yet: no fee");
        assert_eq!(stored_fees(3).await?, vec![Some(vec![40, 7])]);
        assert_eq!(stored_fees(4).await?, vec![None], "all hops or none");
        assert_eq!(
            stored_fees(5).await?,
            vec![Some(vec![30])],
            "nanosecond block time"
        );
        assert_eq!(
            tx_store::get(&tx_hash(6)).await?.unwrap().trade_count,
            Some(0)
        );

        // The same transactions in time order give the same answer.
        clean_tables().await?;
        for (tx, at) in [
            (1, h(2)),
            (2, h(4)),
            (3, h(1)),
            (4, h(2)),
            (5, h(2)),
            (6, h(2)),
        ] {
            seed(tx, at).await?;
        }
        let mut by_time = input.clone();
        by_time.sort_by_key(|(_, p)| p.trades.first().map(|t| t.block_time));
        apply_enriched(&by_time, now).await?;
        assert_eq!(stored_fees(1).await?, vec![Some(vec![30])]);
        assert_eq!(stored_fees(2).await?, vec![None]);
        assert_eq!(stored_fees(3).await?, vec![Some(vec![40, 7])]);
        clear_pool_fee_snapshots().await
    }

    #[tokio::test]
    #[serial(ref_flow_db, pool_info)]
    async fn chunks_cover_every_transaction_and_skip_live_rows() -> Result<()> {
        clean_tables().await?;
        clear_pool_fee_snapshots().await?;
        let now = Utc::now().naive_utc();
        let n = u32::try_from(APPLY_CHUNK_TXS).unwrap() + 1; // forces a second chunk
        let mut receipts = Vec::with_capacity(n as usize);
        for tx in 1..=n {
            receipts.push(
                RefReceipt::new(RefReceiptParts {
                    receipt_id: ReceiptId::new(&id(700_000 + tx)).unwrap(),
                    tx_hash: tx_hash(tx),
                    block_height: 1,
                    block_time: now - TimeDelta::seconds(i64::from(tx)),
                    predecessor: "seed.near".parse().unwrap(),
                    methods: vec!["swap".to_owned()],
                    ok: true,
                })
                .unwrap(),
            );
        }
        ingest_receipts(&receipts).await?;
        // The live collector already enriched one of them.
        tx_store::record_enrichment(
            &tx_hash(n),
            &[trade(
                9_000,
                n,
                now - TimeDelta::seconds(i64::from(n)),
                vec![hop(POOL_A)],
            )],
            now,
        )
        .await?;
        let input: Vec<(TxHash, ParsedTx)> = (1..=n)
            .map(|tx| {
                let at = now - TimeDelta::seconds(i64::from(tx));
                (
                    tx_hash(tx),
                    parsed(vec![trade(tx, tx, at, vec![hop(POOL_A)])]),
                )
            })
            .chain(std::iter::once((tx_hash(n + 1), parsed(vec![])))) // no frame row
            .collect();
        let out = apply_enriched(&input, now).await?;
        assert_eq!(out.recorded, usize::from(n as u16) - 1);
        assert_eq!(out.trades, usize::from(n as u16) - 1);
        assert_eq!(out.skipped_live, 1);
        assert_eq!(out.not_in_frame, 1);
        let live = trade_store::get_by_tx_hash(&tx_hash(n)).await?;
        assert_eq!(live.len(), 1);
        assert_eq!(
            live[0].receipt_id,
            ReceiptId::new(&id(500_000 + 9_000)).unwrap()
        );
        Ok(())
    }
}
