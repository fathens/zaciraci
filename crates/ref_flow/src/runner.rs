//! The long-running collector loop spawned by the backend.
//!
//! Every cron tick: reload config, check the kill switch and the API key,
//! load (or initialise) the state row, build the budget tracker from the
//! ledger and run log, run one [`Cycle`] and persist the state through
//! the compare-and-swap API. A version conflict aborts the cycle: it
//! means another collector instance is writing the same state.

use crate::accounting::{BudgetTracker, DbLedger, StreamStats};
use crate::budget::{self, LEDGER_MIN_KEEP_DAYS};
use crate::client::{DEFAULT_BASE_URL, NearBlocksClient, RetryDelays};
use crate::collector::{Cycle, CycleConfig, CycleReport};
use crate::rate_limit::RateLimiter;
use crate::state::CollectorState;
use anyhow::{Context, Result, anyhow};
use chrono::{NaiveDate, NaiveDateTime, TimeDelta, Utc};
use common::config::{ConfigAccess, ConfigResolver};
use logging::*;
use persistence::ref_flow::runs::{DailyCredits, RunStream};
use persistence::ref_flow::state as state_store;
use persistence::ref_flow::state::{InitOutcome, SaveOutcome};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

/// Fallback when the configured schedule does not parse.
pub const DEFAULT_CRON_SCHEDULE: &str = "0 0,30 * * * *";
/// Hard floor between cycle starts, independent of the configured
/// schedule: a mistyped cron expression must not turn into a request
/// storm.
pub const MIN_CYCLE_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// Entry point for `tokio::spawn`.
pub async fn run(cfg: ConfigResolver) {
    let log = DEFAULT.new(o!("function" => "ref_flow::run"));
    let schedule = parse_schedule(&cfg.ref_flow_cron_schedule(), &log);
    let startup = common::config::startup::get();
    let writer_id = format!("{}/{}", startup.instance_id, std::process::id());
    let mut client: Option<Arc<NearBlocksClient>> = None;
    let mut last_cycle_start: Option<Instant> = None;
    let mut warned_no_key = false;

    for next in schedule.upcoming(Utc) {
        let max_sleep = Duration::from_secs(cfg.cron_max_sleep_seconds());
        common::cron::sleep_until(&next, max_sleep, |_| {}).await;

        if let Err(e) = persistence::config_store::reload_to_config(&startup.instance_id).await {
            warn!(log, "config reload failed, using previous values"; "error" => %e);
        }
        if !cfg.ref_flow_enabled() {
            debug!(log, "collector disabled (REF_FLOW_ENABLED=false)");
            continue;
        }
        if startup.nearblocks_api_key.is_empty() {
            if !warned_no_key {
                error!(
                    log,
                    "REF_FLOW_ENABLED but NEARBLOCKS_API_KEY is unset; collector idle"
                );
                warned_no_key = true;
            }
            continue;
        }
        if let Some(prev) = last_cycle_start
            && prev.elapsed() < MIN_CYCLE_INTERVAL
        {
            warn!(log, "cycle skipped: previous cycle started less than the minimum interval ago";
                "elapsed_s" => prev.elapsed().as_secs());
            continue;
        }
        let client = match &client {
            Some(c) => Arc::clone(c),
            None => match NearBlocksClient::new(
                DEFAULT_BASE_URL,
                &startup.nearblocks_api_key,
                Arc::new(RateLimiter::for_plan(&budget::STANDARD)),
                RetryDelays::default(),
            ) {
                Ok(c) => {
                    let c = Arc::new(c);
                    client = Some(Arc::clone(&c));
                    c
                }
                Err(e) => {
                    error!(log, "cannot build NearBlocks client"; "error" => %e);
                    continue;
                }
            },
        };
        last_cycle_start = Some(Instant::now());
        match run_cycle_once(&cfg, &writer_id, client).await {
            Ok(Some(report)) => {
                info!(log, "cycle finished"; "fwd_kept" => report.forward.kept,
                    "enrich_recorded" => report.enrich.recorded,
                    "rate_limited" => report.rate_limited, "net_down" => report.net_down);
            }
            Ok(None) => {}
            Err(e) => error!(log, "cycle failed"; "error" => format!("{e:#}")),
        }
    }
}

fn parse_schedule(expr: &str, log: &Logger) -> cron::Schedule {
    match expr.parse::<cron::Schedule>() {
        Ok(s) => {
            info!(log, "ref_flow schedule configured"; "schedule" => expr);
            s
        }
        Err(e) => {
            error!(log, "failed to parse ref_flow schedule, using default";
                "error" => ?e, "schedule" => expr, "default" => DEFAULT_CRON_SCHEDULE);
            DEFAULT_CRON_SCHEDULE
                .parse()
                .expect("hardcoded default cron schedule must be valid")
        }
    }
}

/// Load the state row, creating it on first start with the forward
/// cursor at "now" (earlier history is the importer's job).
async fn load_or_init_state(
    writer_id: &str,
    now: NaiveDateTime,
) -> Result<state_store::Versioned<CollectorState>> {
    if let Some(v) = state_store::load::<CollectorState>().await? {
        v.state.validate()?;
        return Ok(v);
    }
    let since_ns = crate::parse::time_to_ns(now);
    match state_store::initialize(writer_id, &CollectorState::new(since_ns), now).await? {
        InitOutcome::Created | InitOutcome::AlreadyInitialized => {}
    }
    let v = state_store::load::<CollectorState>()
        .await?
        .ok_or_else(|| anyhow!("collector state missing right after initialisation"))?;
    v.state.validate()?;
    Ok(v)
}

/// Forward demand and enrich spend from the daily per-stream credit
/// aggregation of `ref_flow_runs`.
pub fn stream_stats_from(rows: &[DailyCredits], today: NaiveDate) -> Result<StreamStats> {
    let mut fwd_by_day: std::collections::BTreeMap<NaiveDate, u64> = Default::default();
    let mut enrich_today = 0u64;
    for row in rows {
        let credits = u64::try_from(row.credits).unwrap_or(0);
        match row.stream()? {
            RunStream::Fwd => *fwd_by_day.entry(row.day).or_insert(0) += credits,
            RunStream::Enrich if row.day == today => enrich_today += credits,
            RunStream::Enrich | RunStream::Gap => {}
        }
    }
    let week_start = today - TimeDelta::days(7);
    let week: u64 = fwd_by_day.range(week_start..today).map(|(_, v)| *v).sum();
    let yesterday = today - TimeDelta::days(1);
    Ok(StreamStats {
        fwd_7d_mean: week / 7,
        fwd_last_day: fwd_by_day.get(&yesterday).copied().unwrap_or(0),
        enrich_used_today: enrich_today,
    })
}

async fn run_cycle_once(
    cfg: &ConfigResolver,
    writer_id: &str,
    client: Arc<NearBlocksClient>,
) -> Result<Option<CycleReport>> {
    let log = DEFAULT.new(o!("function" => "ref_flow::run_cycle_once"));
    let now = Utc::now().naive_utc();
    let today = now.date();

    let loaded = load_or_init_state(writer_id, now).await?;
    let mut state = loaded.state;
    if let Some(until) = state.cooldown_until
        && until > now.and_utc().timestamp()
    {
        info!(log, "in rate-limit cooldown, skipping cycle"; "until" => until);
        return Ok(None);
    }

    let ledger = persistence::ref_flow::ledger::window(
        today - TimeDelta::days(i64::from(LEDGER_MIN_KEEP_DAYS)),
        today,
    )
    .await?;
    let rows = persistence::ref_flow::runs::credits_by_day(now - TimeDelta::days(8)).await?;
    let stats = stream_stats_from(&rows, today)?;
    let tracker = BudgetTracker::new(
        DbLedger,
        &budget::STANDARD,
        ledger,
        stats,
        Some(cfg.ref_flow_day_allow_cap()),
        today,
    );
    let config = CycleConfig {
        retention_days: cfg.ref_flow_retention_days(),
        operator_cap: Some(cfg.ref_flow_day_allow_cap()),
        ..CycleConfig::default()
    };

    // Compare-and-swap saves; the version advances with every save.
    let version = Arc::new(Mutex::new(loaded.version));
    let writer = writer_id.to_owned();
    let mut save = {
        let version = Arc::clone(&version);
        move |s: &CollectorState| -> futures::future::BoxFuture<'static, Result<()>> {
            let snapshot = s.clone();
            let version = Arc::clone(&version);
            let writer = writer.clone();
            Box::pin(async move {
                let expected = *version.lock().unwrap_or_else(|p| p.into_inner());
                match state_store::save(expected, &writer, &snapshot, Utc::now().naive_utc())
                    .await?
                {
                    SaveOutcome::Saved { version: v } => {
                        *version.lock().unwrap_or_else(|p| p.into_inner()) = v;
                        Ok(())
                    }
                    SaveOutcome::Conflict => Err(anyhow!(
                        "collector state version conflict (expected {expected}): another instance is writing"
                    )),
                }
            })
        }
    };

    state.last_cycle_at = Some(now);
    let mut cycle = Cycle::new(client, tracker, config);
    let report = cycle
        .run(&mut state, &mut save)
        .await
        .context("collection cycle")?;
    if !report.rate_limited && !report.net_down {
        state.last_success_at = Some(Utc::now().naive_utc());
    }
    save(&state).await.context("save state at cycle end")?;
    Ok(Some(report))
}

#[cfg(test)]
mod tests;
