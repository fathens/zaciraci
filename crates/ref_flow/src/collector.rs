//! One collection cycle: forward listing, gap catch-up, enrichment and
//! retention cleanup, in that priority order.
//!
//! ```text
//! forward  -> list newest receipts down to the saved cursor (never
//!             deadline-cut; registers a gap if it hits the page cap)
//! gaps     -> recent gaps first (bounded by GAP_TIME_S), old gaps only
//!             with leftover budget and time
//! enrich   -> fetch the receipt trees of pending transactions, newest
//!             30-minute buckets first, round-robin across buckets
//! cleanup  -> retention DELETE
//! ```
//!
//! The cycle is generic over the API (fakes in tests) and the ledger
//! store; persistence of receipts, trades and run rows goes through
//! `persistence::ref_flow` directly.

use crate::accounting::{BudgetTracker, LedgerStore};
use crate::budget::{self, Stream};
use crate::client::{FetchError, ListFrom, MAX_ATTEMPTS, NearBlocksApi};
use crate::parse::{self, ParseStats};
use crate::state::{CollectorState, Gap};
use anyhow::{Context, Result};
use chrono::{NaiveDateTime, TimeDelta, Utc};
use common::types::TxHash;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use logging::*;
use persistence::ref_flow::receipt::{RefReceipt, ingest_receipts};
use persistence::ref_flow::runs::{RunRecord, RunStream};
use persistence::ref_flow::tx::{self as tx_store, EnrichmentOutcome, PendingTx, ReplacePolicy};
use persistence::ref_flow::{self as rf, trade as trade_store};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

/// Forward pass stops after this many pages and leaves a gap.
pub const FWD_MAX_PAGES: u32 = 100;
/// Newest pending transactions considered per cycle.
pub const ENRICH_WINDOW: i64 = 60_000;
/// Enrichment failures before a transaction is isolated.
pub const MAX_ENRICH_ATTEMPTS: u16 = 3;
/// Consecutive network failures that abort the enrich pass.
const MAX_CONSECUTIVE_NETWORK_FAILURES: u32 = 3;
/// Cooldown after the provider rate-limits us.
pub const RATE_LIMIT_COOLDOWN: Duration = Duration::from_secs(55 * 60);
/// Margin between the retention cutoff and the oldest pending candidate,
/// so the daily DELETE and enrichment never race on the same rows.
const PENDING_MARGIN: TimeDelta = TimeDelta::days(1);
const BUCKET: TimeDelta = TimeDelta::minutes(30);

/// Static knobs of a cycle.
#[derive(Debug, Clone)]
pub struct CycleConfig {
    pub retention_days: u32,
    pub operator_cap: Option<u64>,
    pub workers: usize,
    pub run_deadline: Duration,
    pub gap_time: Duration,
    pub fwd_max_pages: u32,
    pub enrich_window: i64,
    pub max_enrich_attempts: u16,
}

impl Default for CycleConfig {
    fn default() -> Self {
        Self {
            retention_days: 90,
            operator_cap: None,
            workers: usize::try_from(budget::STANDARD.workers).unwrap_or(1),
            run_deadline: Duration::from_secs(budget::RUN_DEADLINE_S),
            gap_time: Duration::from_secs(budget::GAP_TIME_S),
            fwd_max_pages: FWD_MAX_PAGES,
            enrich_window: ENRICH_WINDOW,
            max_enrich_attempts: MAX_ENRICH_ATTEMPTS,
        }
    }
}

/// Why a pass stopped early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Budget,
    Deadline,
    RateLimited,
    NetworkDown,
    PageCap,
}

/// Outcome of one list walk (forward or gap).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListWalk {
    pub cursor: Option<String>,
    pub pages: u32,
    pub kept: u32,
    pub parse_errors: u32,
    pub newest_ns: Option<u64>,
    pub oldest_ns: Option<u64>,
    /// Reached `until_ns` or the end of the list.
    pub complete: bool,
    pub failed: bool,
    pub stop: Option<Stop>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnrichSummary {
    pub fetched: u32,
    pub recorded: u32,
    pub trades: u32,
    pub failed: u32,
    pub isolated: u32,
    pub candidates: usize,
    pub stop: Option<Stop>,
    pub stats: ParseStats,
}

#[derive(Debug, Clone, Default)]
pub struct CycleReport {
    pub forward: ListWalk,
    pub recent_gaps: u32,
    pub enrich: EnrichSummary,
    pub old_gaps: u32,
    pub rate_limited: bool,
    pub net_down: bool,
}

/// Everything a cycle needs besides the mutable state.
pub struct Cycle<A: NearBlocksApi + 'static, L: LedgerStore> {
    pub api: Arc<A>,
    pub tracker: BudgetTracker<L>,
    pub config: CycleConfig,
    started: Instant,
    log: Logger,
}

impl<A: NearBlocksApi + 'static, L: LedgerStore> Cycle<A, L> {
    pub fn new(api: Arc<A>, tracker: BudgetTracker<L>, config: CycleConfig) -> Self {
        Self {
            api,
            tracker,
            config,
            started: Instant::now(),
            log: DEFAULT.new(o!("function" => "ref_flow::cycle")),
        }
    }

    fn deadline(&self) -> Instant {
        self.started + self.config.run_deadline
    }

    fn now() -> NaiveDateTime {
        Utc::now().naive_utc()
    }

    /// Run every pass. `state` is updated in place; the caller persists it
    /// after the forward pass (via `save_state`) and at the end.
    pub async fn run<S>(
        &mut self,
        state: &mut CollectorState,
        save_state: &mut S,
    ) -> Result<CycleReport>
    where
        S: FnMut(&CollectorState) -> futures::future::BoxFuture<'static, Result<()>>,
    {
        let mut report = CycleReport::default();
        info!(self.log, "cycle start"; "report" => %self.tracker.report(Self::now()),
            "fwd_need" => self.tracker.fwd_need(), "gaps" => state.gaps.len());

        report.forward = self.forward_pass(state).await?;
        save_state(state)
            .await
            .context("save state after forward")?;

        let recent_cut = Self::now() - TimeDelta::hours(72);
        report.recent_gaps = self.gaps_pass(state, recent_cut, true).await?;
        save_state(state)
            .await
            .context("save state after recent gaps")?;

        report.enrich = self.enrich_pass().await?;

        if self.tracker.left(Stream::Gap, Self::now()) > 0 && Instant::now() < self.deadline() {
            report.old_gaps = self.gaps_pass(state, recent_cut, false).await?;
            save_state(state)
                .await
                .context("save state after old gaps")?;
        }

        self.cleanup().await;

        report.rate_limited = self.tracker.is_rate_limited();
        report.net_down = self.tracker.is_net_down();
        if report.rate_limited {
            let until = Utc::now() + RATE_LIMIT_COOLDOWN;
            state.cooldown_until = Some(until.timestamp());
            warn!(self.log, "rate limited: cooling down"; "until" => %until);
        }
        info!(self.log, "cycle done";
            "calls" => self.tracker.calls_this_run(),
            "fwd_pages" => report.forward.pages, "fwd_kept" => report.forward.kept,
            "enrich_fetched" => report.enrich.fetched, "enrich_trades" => report.enrich.trades,
            "enrich_failed" => report.enrich.failed, "enrich_stop" => ?report.enrich.stop,
            "report" => %self.tracker.report(Self::now()));
        Ok(report)
    }

    // ------------------------------------------------------------ list

    /// Walk the receipt list from `from` towards older receipts until a
    /// receipt at or before `until_ns` is seen, the list ends, `max_pages`
    /// is reached, the budget of `stream` runs out, or `deadline` passes
    /// (`None`: never deadline-cut).
    async fn list_walk(
        &mut self,
        stream: Stream,
        from: ListFrom,
        until_ns: u64,
        max_pages: u32,
        deadline: Option<Instant>,
    ) -> Result<ListWalk> {
        let mut walk = ListWalk::default();
        let mut from = from;
        let credits = self.tracker.credits();
        loop {
            if walk.pages >= max_pages {
                walk.stop = Some(Stop::PageCap);
                break;
            }
            if deadline.is_some_and(|d| Instant::now() >= d) {
                walk.stop = Some(Stop::Deadline);
                break;
            }
            if self
                .tracker
                .ensure_credits(stream, MAX_ATTEMPTS, Self::now())
                .await?
                < MAX_ATTEMPTS
            {
                walk.stop = Some(Stop::Budget);
                break;
            }
            let page = match self.api.list_receipts(&from, &*credits).await {
                Ok(page) => page,
                Err(FetchError::RateLimited) => {
                    self.tracker.set_rate_limited();
                    walk.failed = true;
                    walk.stop = Some(Stop::RateLimited);
                    break;
                }
                Err(FetchError::Network(e)) => {
                    warn!(self.log, "list fetch failed"; "error" => %e);
                    self.tracker.set_net_down();
                    walk.failed = true;
                    walk.stop = Some(Stop::NetworkDown);
                    break;
                }
                Err(e) => {
                    warn!(self.log, "list fetch failed"; "error" => %e);
                    walk.failed = true;
                    break;
                }
            };
            if page.items.is_empty() {
                walk.complete = true;
                break;
            }
            walk.pages += 1;
            let mut receipts: Vec<RefReceipt> = Vec::with_capacity(page.items.len());
            for item in &page.items {
                let Some(ts) = parse::list_item_timestamp_ns(item) else {
                    walk.parse_errors += 1;
                    continue;
                };
                walk.newest_ns = Some(walk.newest_ns.map_or(ts, |n| n.max(ts)));
                walk.oldest_ns = Some(walk.oldest_ns.map_or(ts, |o| o.min(ts)));
                if ts <= until_ns {
                    walk.complete = true;
                    continue;
                }
                match parse::parse_list_item(item) {
                    Ok(Some(receipt)) => receipts.push(receipt),
                    Ok(None) => {}
                    Err(e) => {
                        walk.parse_errors += 1;
                        debug!(self.log, "list item rejected"; "error" => %e);
                    }
                }
            }
            if !receipts.is_empty() {
                let stats = ingest_receipts(&receipts).await?;
                walk.kept += u32::try_from(stats.receipts_inserted).unwrap_or(u32::MAX);
            }
            match (walk.complete, page.next_cursor) {
                (true, cursor) => {
                    walk.cursor = cursor;
                    break;
                }
                (false, None) => {
                    walk.complete = true;
                    break;
                }
                (false, Some(cursor)) => {
                    walk.cursor = Some(cursor.clone());
                    from = ListFrom::Cursor(cursor);
                }
            }
        }
        Ok(walk)
    }

    async fn forward_pass(&mut self, state: &mut CollectorState) -> Result<ListWalk> {
        let since_ns = state.since_ns;
        let credits_at_start = self.tracker.calls_this_run();
        let walk = self
            .list_walk(
                Stream::Forward,
                ListFrom::Newest,
                since_ns,
                self.config.fwd_max_pages,
                None,
            )
            .await?;
        if let Some(newest) = walk.newest_ns {
            // The cursor moves to the newest receipt seen, so whatever the
            // walk did not reach (page cap, budget, or a failure after the
            // first page) must be registered as a gap or it is lost.
            if !walk.complete
                && let Err(e) = state.push_gap(Gap {
                    cursor: walk.cursor.clone(),
                    until_ns: since_ns,
                    oldest_ns: walk.oldest_ns,
                })
            {
                error!(self.log, "cannot register forward gap"; "error" => %e);
            }
            state.since_ns = state.since_ns.max(newest);
        }
        self.record_run(
            RunStream::Fwd,
            &walk,
            credits_at_start,
            format!("complete={}", u8::from(walk.complete)),
        )
        .await;
        info!(self.log, "forward done"; "pages" => walk.pages, "kept" => walk.kept,
            "complete" => walk.complete, "stop" => ?walk.stop);
        Ok(walk)
    }

    /// Catch up gaps. `recent == true` handles gaps whose newest edge is at
    /// or after `recent_cut` (forward continuations), bounded by
    /// `gap_time`; `recent == false` handles the older ones until the run
    /// deadline. Returns the number of gaps closed.
    async fn gaps_pass(
        &mut self,
        state: &mut CollectorState,
        recent_cut: NaiveDateTime,
        recent: bool,
    ) -> Result<u32> {
        let recent_cut_ns =
            u64::try_from(recent_cut.and_utc().timestamp_nanos_opt().unwrap_or(0)).unwrap_or(0);
        let limit = if recent {
            self.started + self.config.gap_time
        } else {
            self.deadline()
        };
        let mut gaps = std::mem::take(&mut state.gaps);
        gaps.sort_by_key(Gap::span_ns);
        let mut remaining = Vec::with_capacity(gaps.len());
        let mut closed_count = 0;
        for mut gap in gaps {
            let is_recent = gap.until_ns >= recent_cut_ns;
            if is_recent != recent
                || self.tracker.is_rate_limited()
                || self.tracker.is_net_down()
                || Instant::now() >= limit
            {
                remaining.push(gap);
                continue;
            }
            let from = match (&gap.cursor, gap.oldest_ns) {
                (Some(cursor), _) => ListFrom::Cursor(cursor.clone()),
                (None, Some(oldest)) => ListFrom::BeforeNs(oldest.saturating_add(1)),
                (None, None) => ListFrom::Newest,
            };
            let credits_at_start = self.tracker.calls_this_run();
            let walk = self
                .list_walk(Stream::Gap, from, gap.until_ns, u32::MAX, Some(limit))
                .await?;
            let closed = walk.complete && !walk.failed;
            if walk.pages > 0 {
                gap.cursor = walk.cursor.clone();
                if walk.oldest_ns.is_some() {
                    gap.oldest_ns = walk.oldest_ns;
                }
            }
            if closed {
                closed_count += 1;
            } else {
                remaining.push(gap.clone());
            }
            self.record_run(
                RunStream::Gap,
                &walk,
                credits_at_start,
                format!("closed={}", u8::from(closed)),
            )
            .await;
            if walk.pages > 0 || closed {
                info!(self.log, "gap pass"; "pages" => walk.pages, "closed" => closed,
                    "left_hours" => gap.span_ns() as f64 / 3.6e12, "stop" => ?walk.stop);
            }
        }
        state.gaps = remaining;
        Ok(closed_count)
    }

    /// Record one list pass; `credits_at_start` is the cycle's charge
    /// counter before the pass so the row carries the pass's own spend.
    async fn record_run(
        &self,
        stream: RunStream,
        walk: &ListWalk,
        credits_at_start: u64,
        note: String,
    ) {
        let credits = self
            .tracker
            .calls_this_run()
            .saturating_sub(credits_at_start);
        let record = RunRecord {
            run_at: Self::now(),
            stream,
            pages: walk.pages,
            receipts: walk.kept,
            trades: 0,
            from_time: walk.oldest_ns.and_then(ns_to_time),
            to_time: walk.newest_ns.and_then(ns_to_time),
            credits: u32::try_from(credits).unwrap_or(u32::MAX),
            http429: 0,
            failures: u32::from(walk.failed) + walk.parse_errors,
            note,
            parse_stats: None,
        };
        if let Err(e) = persistence::ref_flow::runs::insert(&record).await {
            warn!(self.log, "failed to record run"; "error" => %e);
        }
    }

    // ---------------------------------------------------------- enrich

    async fn enrich_pass(&mut self) -> Result<EnrichSummary> {
        let mut summary = EnrichSummary::default();
        let now = Self::now();
        let not_before =
            now - TimeDelta::days(i64::from(self.config.retention_days)) + PENDING_MARGIN;
        let candidates =
            tx_store::pending_candidates(not_before, self.config.enrich_window).await?;
        summary.candidates = candidates.len();
        let mut order = round_robin_by_bucket(candidates);
        order.reverse(); // pop() from the back
        let credits_at_start = self.tracker.calls_this_run();
        let credits = self.tracker.credits();
        let api = Arc::clone(&self.api);
        let mut in_flight: FuturesUnordered<FetchTask> = FuturesUnordered::new();
        let mut consecutive_network_failures = 0u32;
        let mut stats = ParseStats::default();
        let max_attempts = self.config.max_enrich_attempts;

        loop {
            // Dispatch while workers are free, credits are reserved for
            // every in-flight attempt and nothing says stop.
            while summary.stop.is_none() && in_flight.len() < self.config.workers {
                if Instant::now() >= self.deadline() {
                    summary.stop = Some(Stop::Deadline);
                    break;
                }
                let needed = MAX_ATTEMPTS * (u32::try_from(in_flight.len()).unwrap_or(0) + 1);
                if self
                    .tracker
                    .ensure_credits(Stream::Enrich, needed, Self::now())
                    .await?
                    < needed
                {
                    if in_flight.is_empty() {
                        summary.stop = Some(Stop::Budget);
                    }
                    break;
                }
                let Some(candidate) = order.pop() else { break };
                let api = Arc::clone(&api);
                let credits = Arc::clone(&credits);
                in_flight.push(tokio::spawn(async move {
                    let result = api.txn_receipts(&candidate.tx_hash, &*credits).await;
                    (candidate, result)
                }));
            }
            let Some(joined) = in_flight.next().await else {
                break;
            };
            let (candidate, result) = joined.context("enrich worker panicked")?;
            match result {
                Ok(roots) => {
                    consecutive_network_failures = 0;
                    summary.fetched += 1;
                    self.record_enrichment(
                        &candidate.tx_hash,
                        &roots,
                        &mut summary,
                        &mut stats,
                        max_attempts,
                    )
                    .await?;
                }
                Err(FetchError::RateLimited) => {
                    self.tracker.set_rate_limited();
                    summary.stop = Some(Stop::RateLimited);
                }
                Err(FetchError::NoCredit) => {
                    summary.stop.get_or_insert(Stop::Budget);
                }
                Err(FetchError::Network(e)) => {
                    consecutive_network_failures += 1;
                    warn!(self.log, "enrich fetch failed"; "error" => %e);
                    if consecutive_network_failures >= MAX_CONSECUTIVE_NETWORK_FAILURES {
                        self.tracker.set_net_down();
                        summary.stop = Some(Stop::NetworkDown);
                    }
                }
                Err(e) => {
                    // A definitive provider answer that is not a receipt
                    // tree: count it against the transaction.
                    consecutive_network_failures = 0;
                    summary.failed += 1;
                    warn!(self.log, "enrich response unusable"; "error" => %e);
                    self.fail_attempt(&candidate.tx_hash, &mut summary, max_attempts)
                        .await?;
                }
            }
        }
        summary.stats = stats;
        let record = RunRecord {
            run_at: Self::now(),
            stream: RunStream::Enrich,
            pages: summary.fetched,
            receipts: summary.recorded,
            trades: summary.trades,
            from_time: None,
            to_time: None,
            credits: u32::try_from(self.tracker.calls_this_run() - credits_at_start)
                .unwrap_or(u32::MAX),
            http429: 0,
            failures: summary.failed,
            note: format!("candidates={} stop={:?}", summary.candidates, summary.stop),
            parse_stats: Some(summary.stats.to_json()),
        };
        if let Err(e) = persistence::ref_flow::runs::insert(&record).await {
            warn!(self.log, "failed to record run"; "error" => %e);
        }
        info!(self.log, "enrich done"; "candidates" => summary.candidates, "fetched" => summary.fetched,
            "recorded" => summary.recorded, "trades" => summary.trades, "failed" => summary.failed,
            "isolated" => summary.isolated, "stop" => ?summary.stop);
        Ok(summary)
    }

    async fn record_enrichment(
        &self,
        tx_hash: &TxHash,
        roots: &[serde_json::Value],
        summary: &mut EnrichSummary,
        stats: &mut ParseStats,
        max_attempts: u16,
    ) -> Result<()> {
        let parsed = match parse::parse_tx_receipts(tx_hash, roots) {
            Ok(parsed) => parsed,
            Err(e) => {
                summary.failed += 1;
                warn!(self.log, "receipt tree unusable"; "error" => %e);
                return self.fail_attempt(tx_hash, summary, max_attempts).await;
            }
        };
        stats.merge(&parsed.stats);
        let mut trades = Vec::with_capacity(parsed.trades.len());
        for trade in parsed.trades {
            let trade = match trade_store::lookup_hop_fees(trade.hops(), trade.block_time).await? {
                Some(fees) => trade.with_fees(fees).context("attach pool fees")?,
                None => trade,
            };
            trades.push(trade);
        }
        match tx_store::record_enrichment(tx_hash, &trades, ReplacePolicy::Always, Self::now())
            .await?
        {
            EnrichmentOutcome::Recorded { trades: n } => {
                summary.recorded += 1;
                summary.trades += u32::try_from(n).unwrap_or(u32::MAX);
            }
            EnrichmentOutcome::Skipped | EnrichmentOutcome::NotInFrame => {
                // Retention removed the frame row while we were fetching.
                debug!(self.log, "enrichment not recorded: frame row gone");
            }
        }
        Ok(())
    }

    async fn fail_attempt(
        &self,
        tx_hash: &TxHash,
        summary: &mut EnrichSummary,
        max_attempts: u16,
    ) -> Result<()> {
        if let Some(outcome) =
            tx_store::mark_failed_attempt(tx_hash, max_attempts, Self::now()).await?
            && outcome.isolated
        {
            summary.isolated += 1;
            warn!(self.log, "transaction isolated after repeated failures"; "attempts" => outcome.attempts);
        }
        Ok(())
    }

    // --------------------------------------------------------- cleanup

    async fn cleanup(&self) {
        match rf::cleanup_old_records(self.config.retention_days).await {
            Ok(stats) => {
                if stats.txs_deleted > 0 || stats.receipts_deleted > 0 {
                    info!(self.log, "retention cleanup"; "txs" => stats.txs_deleted, "receipts" => stats.receipts_deleted);
                }
            }
            Err(e) => error!(self.log, "retention cleanup failed"; "error" => %e),
        }
        let effective = self.config.retention_days.max(rf::MIN_RETENTION_DAYS);
        let cutoff = Self::now() - TimeDelta::days(i64::from(effective));
        if let Err(e) = persistence::ref_flow::runs::delete_before(cutoff).await {
            error!(self.log, "run log cleanup failed"; "error" => %e);
        }
    }
}

/// A spawned enrich fetch: the candidate and the provider's answer.
type FetchTask = tokio::task::JoinHandle<(PendingTx, Result<Vec<serde_json::Value>, FetchError>)>;

fn ns_to_time(ns: u64) -> Option<NaiveDateTime> {
    let secs = i64::try_from(ns / 1_000_000_000).ok()?;
    let nanos = u32::try_from(ns % 1_000_000_000).ok()?;
    chrono::DateTime::from_timestamp(secs, nanos).map(|t| t.naive_utc())
}

/// Order candidates so that every 30-minute bucket progresses evenly:
/// buckets newest first, one transaction from each bucket per round,
/// deterministic within a bucket (by hash) so partial enrichment carries
/// no time bias.
pub fn round_robin_by_bucket(candidates: Vec<PendingTx>) -> Vec<PendingTx> {
    let mut buckets: BTreeMap<i64, Vec<PendingTx>> = BTreeMap::new();
    for c in candidates {
        let key = c
            .block_time
            .and_utc()
            .timestamp()
            .div_euclid(BUCKET.num_seconds());
        buckets.entry(key).or_default().push(c);
    }
    let mut queues: Vec<std::collections::VecDeque<PendingTx>> = buckets
        .into_values()
        .rev()
        .map(|mut v| {
            v.sort_by(|a, b| a.tx_hash.as_str().cmp(b.tx_hash.as_str()));
            v.into()
        })
        .collect();
    let total: usize = queues.iter().map(|q| q.len()).sum();
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        for q in &mut queues {
            if let Some(c) = q.pop_front() {
                out.push(c);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
