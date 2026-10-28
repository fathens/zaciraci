//! One-off migration of the Python collector's raw archive
//! (`.tmp/ref_flow/raw/*.jsonl.gz`) into the `ref_flow_*` tables.
//!
//! Temporary code: delete once the cutover is complete. Only enabled with
//! the `importer` cargo feature so the backend never links it.
//!
//! The archive has two kinds of files, both one JSON object per line:
//! - list pages (`*_fwd.jsonl.gz`, `*_gap.jsonl.gz`, `*_bf.jsonl.gz`): API
//!   list items, parsed with the same code as the live collector;
//! - enrich results (`*_enrich.jsonl.gz`): one top-level receipt tree per
//!   line, tagged with `_tx_hash` by the Python collector.
//!
//! Rows written by the live collector always win: receipts and frame rows
//! use `ON CONFLICT DO NOTHING`, trades use `ReplacePolicy::OnlyIfPending`.
//! The importer never touches the state, ledger or run tables.

use crate::parse::{self, ParsedTx};
use anyhow::{Context, Result};
use chrono::{NaiveDate, NaiveDateTime};
use common::types::{ReceiptId, TxHash};
use dex::Trade;
use flate2::read::MultiGzDecoder;
use persistence::ref_flow::receipt::{RefReceipt, ingest_receipts};
use persistence::ref_flow::trade as trade_store;
use persistence::ref_flow::tx::{self as tx_store, EnrichmentOutcome, ReplacePolicy};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

/// Which kind of archive file a path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawKind {
    /// Receipt list pages (forward, gap or backfill).
    List,
    /// Transaction receipt trees.
    Enrich,
}

/// Classify by the Python collector's file name convention
/// `ref_<UTC stamp>_<tag>.jsonl.gz`.
pub fn classify(path: &Path) -> Option<RawKind> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl.gz")?;
    let tag = stem.rsplit('_').next()?;
    match tag {
        "enrich" => Some(RawKind::Enrich),
        "fwd" | "gap" | "bf" | "receipts" => Some(RawKind::List),
        _ => None,
    }
}

/// Read every JSON line of a gzip file. A truncated gzip (the Python
/// collector was sometimes killed mid-write) yields the complete lines
/// read so far and reports the truncation.
pub fn read_lines(path: &Path) -> Result<(Vec<Value>, bool)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = BufReader::new(MultiGzDecoder::new(file));
    let mut lines = Vec::new();
    let mut truncated = false;
    let mut buf = String::new();
    loop {
        buf.clear();
        match reader.read_line(&mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let line = buf.trim_end();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(line) {
                    Ok(v) => lines.push(v),
                    Err(_) => truncated = true, // partial last line
                }
            }
            Err(_) => {
                truncated = true;
                break;
            }
        }
    }
    // Drain to detect a corrupt trailer without failing.
    let mut sink = [0u8; 1024];
    while matches!(reader.read(&mut sink), Ok(n) if n > 0) {}
    Ok((lines, truncated))
}

/// Inclusive date range filter on receipt block time (UTC).
#[derive(Debug, Clone, Copy)]
pub struct DateRange {
    pub since: NaiveDate,
    pub until: NaiveDate,
}

impl DateRange {
    pub fn contains(&self, t: NaiveDateTime) -> bool {
        let d = t.date();
        self.since <= d && d <= self.until
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ListSummary {
    pub files: usize,
    pub truncated_files: usize,
    pub lines: usize,
    pub receipts: usize,
    pub ok_receipts: usize,
    pub out_of_range: usize,
    pub skipped_other: usize,
    pub parse_errors: usize,
}

/// Parse list files into receipts inside `range`, deduplicated by id.
pub fn load_receipts(
    files: &[PathBuf],
    range: DateRange,
) -> Result<(Vec<RefReceipt>, ListSummary)> {
    let mut summary = ListSummary::default();
    let mut by_id: HashMap<ReceiptId, RefReceipt> = HashMap::new();
    for path in files {
        summary.files += 1;
        let (lines, truncated) = read_lines(path)?;
        summary.truncated_files += usize::from(truncated);
        for line in lines {
            summary.lines += 1;
            match parse::parse_list_item(&line) {
                Ok(Some(r)) => {
                    if !range.contains(r.block_time) {
                        summary.out_of_range += 1;
                        continue;
                    }
                    by_id.entry(r.receipt_id.clone()).or_insert(r);
                }
                Ok(None) => summary.skipped_other += 1,
                Err(_) => summary.parse_errors += 1,
            }
        }
    }
    let mut receipts: Vec<RefReceipt> = by_id.into_values().collect();
    receipts.sort_by(|a, b| {
        (a.block_time, a.receipt_id.as_str()).cmp(&(b.block_time, b.receipt_id.as_str()))
    });
    summary.receipts = receipts.len();
    summary.ok_receipts = receipts.iter().filter(|r| r.ok).count();
    Ok((receipts, summary))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EnrichSummary {
    pub files: usize,
    pub truncated_files: usize,
    pub lines: usize,
    pub lines_without_tx: usize,
    pub txs: usize,
    pub trades: usize,
    pub rejects: usize,
    pub out_of_range: usize,
    pub stats: parse::ParseStats,
}

/// One transaction's receipt trees from the enrich archive.
#[derive(Debug, Clone)]
pub struct EnrichedTx {
    pub tx_hash: TxHash,
    pub roots: Vec<Value>,
}

/// Group enrich lines by their `_tx_hash` tag. Lines without a valid tag
/// (pre-2026-09-03 archive) are counted and skipped.
pub fn load_enriched(files: &[PathBuf]) -> Result<(Vec<EnrichedTx>, EnrichSummary)> {
    let mut summary = EnrichSummary::default();
    let mut by_tx: BTreeMap<TxHash, Vec<Value>> = BTreeMap::new();
    for path in files {
        summary.files += 1;
        let (lines, truncated) = read_lines(path)?;
        summary.truncated_files += usize::from(truncated);
        for line in lines {
            summary.lines += 1;
            let Some(tx_hash) = line
                .get("_tx_hash")
                .and_then(Value::as_str)
                .and_then(|tx| TxHash::new(tx).ok())
            else {
                summary.lines_without_tx += 1;
                continue;
            };
            by_tx.entry(tx_hash).or_default().push(line);
        }
    }
    let txs = by_tx
        .into_iter()
        .map(|(tx_hash, roots)| EnrichedTx { tx_hash, roots })
        .collect::<Vec<_>>();
    summary.txs = txs.len();
    Ok((txs, summary))
}

/// Parse every enriched transaction into trades, keeping only trades whose
/// receipt time is inside `range`.
pub fn parse_enriched(
    txs: &[EnrichedTx],
    range: DateRange,
    summary: &mut EnrichSummary,
) -> Vec<(TxHash, ParsedTx)> {
    let mut out = Vec::with_capacity(txs.len());
    for tx in txs {
        match parse::parse_tx_receipts(&tx.tx_hash, &tx.roots) {
            Ok(mut parsed) => {
                let before = parsed.trades.len();
                parsed.trades.retain(|t| range.contains(t.block_time));
                summary.out_of_range += before - parsed.trades.len();
                summary.trades += parsed.trades.len();
                summary.rejects += parsed.rejects.len();
                summary.stats.merge(&parsed.stats);
                out.push((tx.tx_hash.clone(), parsed));
            }
            Err(_) => summary.rejects += 1,
        }
    }
    out
}

/// Three-way comparison of parsed trades with the Python `trades.csv`
/// (`receipt_id,...,token_in,amount_in,token_out,amount_out`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Comparison {
    /// Same receipt, same endpoints and amounts.
    pub matched: usize,
    /// Parsed here, absent from the CSV (expected: ft→near_withdraw and
    /// swap+withdraw batches the Python extractor dropped).
    pub new_here: usize,
    /// Present in both with different amounts or tokens.
    pub mismatched: Vec<String>,
    /// In the CSV but not parsed here.
    pub csv_only: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvTrade {
    pub token_in: String,
    pub amount_in: String,
    pub token_out: String,
    pub amount_out: String,
}

/// Load `trades.csv` rows keyed by receipt id.
pub fn load_trades_csv(path: &Path) -> Result<HashMap<String, CsvTrade>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().context("empty csv")?.split(',').collect();
    let idx = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .with_context(|| format!("csv lacks column {name}"))
    };
    let (i_rid, i_tin, i_ain, i_tout, i_aout) = (
        idx("receipt_id")?,
        idx("token_in")?,
        idx("amount_in")?,
        idx("token_out")?,
        idx("amount_out")?,
    );
    let mut out = HashMap::new();
    for line in lines {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() <= i_aout.max(i_rid).max(i_tin).max(i_ain).max(i_tout) {
            continue;
        }
        out.insert(
            cols[i_rid].to_owned(),
            CsvTrade {
                token_in: cols[i_tin].to_owned(),
                amount_in: cols[i_ain].to_owned(),
                token_out: cols[i_tout].to_owned(),
                amount_out: cols[i_aout].to_owned(),
            },
        );
    }
    Ok(out)
}

pub fn compare(trades: &[&Trade], csv: &HashMap<String, CsvTrade>) -> Comparison {
    let mut cmp = Comparison::default();
    let mut seen = std::collections::HashSet::new();
    for t in trades {
        let rid = t.receipt_id.as_str();
        seen.insert(rid.to_owned());
        match csv.get(rid) {
            None => cmp.new_here += 1,
            Some(c) => {
                let same = c.token_in == t.token_in().to_string()
                    && c.token_out == t.token_out().to_string()
                    && c.amount_in == t.amount_in().to_string()
                    && c.amount_out == t.amount_out().to_string();
                if same {
                    cmp.matched += 1;
                } else {
                    cmp.mismatched.push(rid.to_owned());
                }
            }
        }
    }
    cmp.csv_only = csv.keys().filter(|k| !seen.contains(*k)).count();
    cmp
}

/// Write receipts (and their frame rows) in batches; duplicates are
/// skipped by the DB.
pub async fn apply_receipts(receipts: &[RefReceipt], batch: usize) -> Result<(usize, usize)> {
    let mut inserted = 0;
    let mut txs = 0;
    for chunk in receipts.chunks(batch.max(1)) {
        let stats = ingest_receipts(chunk).await?;
        inserted += stats.receipts_inserted;
        txs += stats.txs_upserted;
    }
    Ok((inserted, txs))
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ApplyEnrichSummary {
    pub recorded: usize,
    pub trades: usize,
    pub skipped_live: usize,
    pub not_in_frame: usize,
}

/// Record parsed trades for transactions still pending. Pool fees are
/// attached from `pool_info` when a snapshot exists.
pub async fn apply_enriched(
    parsed: &[(TxHash, ParsedTx)],
    now: NaiveDateTime,
) -> Result<ApplyEnrichSummary> {
    let mut summary = ApplyEnrichSummary::default();
    for (tx_hash, parsed) in parsed {
        let mut trades = Vec::with_capacity(parsed.trades.len());
        for trade in &parsed.trades {
            let trade = match trade_store::lookup_hop_fees(trade.hops(), trade.block_time).await? {
                Some(fees) => trade.clone().with_fees(fees).context("attach pool fees")?,
                None => trade.clone(),
            };
            trades.push(trade);
        }
        match tx_store::record_enrichment(tx_hash, &trades, ReplacePolicy::OnlyIfPending, now)
            .await?
        {
            EnrichmentOutcome::Recorded { trades: n } => {
                summary.recorded += 1;
                summary.trades += n;
            }
            EnrichmentOutcome::Skipped => summary.skipped_live += 1,
            EnrichmentOutcome::NotInFrame => summary.not_in_frame += 1,
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests;
