#![deny(warnings)]

//! One-off import of the Python collector's raw archive into the
//! `ref_flow_*` tables. Dry-run by default; `--apply` writes.
//!
//! ```text
//! DATABASE_URL=postgres://... ref_flow_import \
//!     --raw-dir .tmp/ref_flow/raw --since 2026-09-10 --until 2026-10-25 \
//!     [--compare-csv .tmp/ref_flow/analysis/trades.csv] [--apply]
//! ```
//!
//! Rows written by the live collector win (receipts/frames: ON CONFLICT
//! DO NOTHING; trades: only for transactions still pending). State,
//! ledger and run tables are never touched.

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::Parser;
use logging::*;
use ref_flow::import::{self, DateRange, RawKind};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "ref_flow_import",
    about = "Import the Python ref_flow raw archive into ref_flow_* tables"
)]
struct Args {
    /// Directory with ref_*.jsonl.gz files.
    #[arg(long)]
    raw_dir: PathBuf,
    /// First UTC day (inclusive) of receipt block time to import.
    #[arg(long)]
    since: NaiveDate,
    /// Last UTC day (inclusive).
    #[arg(long)]
    until: NaiveDate,
    /// Compare parsed trades with the Python trades.csv (dry-run aid).
    #[arg(long)]
    compare_csv: Option<PathBuf>,
    /// Write to the database (default: report only).
    #[arg(long)]
    apply: bool,
    /// Receipts per INSERT batch.
    #[arg(long, default_value_t = 5_000)]
    batch: usize,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let log = DEFAULT.new(o!("function" => "ref_flow_import"));
    if args.since > args.until {
        bail!("--since must not be after --until");
    }
    let range = DateRange {
        since: args.since,
        until: args.until,
    };
    if args.apply {
        let url = common::config::startup::get().database_url.clone();
        let host = url.split('@').nth(1).unwrap_or("?").to_owned();
        warn!(log, "APPLY mode: writing to the database"; "target" => host, "since" => %args.since, "until" => %args.until);
    } else {
        info!(log, "dry run (pass --apply to write)"; "since" => %args.since, "until" => %args.until);
    }

    let mut list_files = Vec::new();
    let mut enrich_files = Vec::new();
    for entry in std::fs::read_dir(&args.raw_dir)
        .with_context(|| format!("read {}", args.raw_dir.display()))?
    {
        let path = entry?.path();
        match import::classify(&path) {
            Some(RawKind::List) => list_files.push(path),
            Some(RawKind::Enrich) => enrich_files.push(path),
            None => {}
        }
    }
    list_files.sort();
    enrich_files.sort();
    info!(log, "archive scanned"; "list_files" => list_files.len(), "enrich_files" => enrich_files.len());

    let (receipts, list_summary) = import::load_receipts(&list_files, range)?;
    info!(log, "list files parsed";
        "lines" => list_summary.lines, "receipts" => list_summary.receipts,
        "ok_receipts" => list_summary.ok_receipts, "out_of_range" => list_summary.out_of_range,
        "skipped_other" => list_summary.skipped_other, "parse_errors" => list_summary.parse_errors,
        "truncated_files" => list_summary.truncated_files);

    let (txs, mut enrich_summary) = import::load_enriched(&enrich_files)?;
    let parsed = import::parse_enriched(&txs, range, &mut enrich_summary);
    info!(log, "enrich files parsed";
        "lines" => enrich_summary.lines, "lines_without_tx" => enrich_summary.lines_without_tx,
        "txs" => enrich_summary.txs, "trades" => enrich_summary.trades,
        "rejects" => enrich_summary.rejects, "out_of_range" => enrich_summary.out_of_range,
        "truncated_files" => enrich_summary.truncated_files,
        "parse_stats" => %enrich_summary.stats.to_json());

    if let Some(csv_path) = &args.compare_csv {
        let csv = import::load_trades_csv(csv_path)?;
        let trades: Vec<&dex::Trade> = parsed.iter().flat_map(|(_, p)| p.trades.iter()).collect();
        let cmp = import::compare(&trades, &csv);
        info!(log, "comparison with trades.csv";
            "matched" => cmp.matched, "new_here" => cmp.new_here,
            "mismatched" => cmp.mismatched.len(), "csv_only" => cmp.csv_only);
        for rid in cmp.mismatched.iter().take(20) {
            warn!(log, "mismatch"; "receipt_id" => rid);
        }
    }

    if !args.apply {
        info!(log, "dry run complete");
        return Ok(());
    }

    let (inserted, frames) = import::apply_receipts(&receipts, args.batch).await?;
    info!(log, "receipts written"; "inserted" => inserted, "frame_rows_upserted" => frames);
    let applied = import::apply_enriched(&parsed, chrono::Utc::now().naive_utc()).await?;
    info!(log, "enrichment written";
        "recorded" => applied.recorded, "trades" => applied.trades,
        "skipped_live" => applied.skipped_live, "not_in_frame" => applied.not_in_frame);
    Ok(())
}
