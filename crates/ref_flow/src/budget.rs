//! Request budget: constants and pure functions only (no I/O, no clock).
//!
//! Port of the Python collector's `budget.py` (frozen as
//! `tests/fixtures/generators/budget_reference.py`) restricted to the
//! NearBlocks *Standard* plan. Policy summary:
//!
//! ```text
//! day_allow = min(DAY_MAX, operator cap)
//! R(t)      = max(RESERVE_MIN, int(min(0.9*day_allow, max(NEED, fwd_need)) * (86400 - sec_of_day) / 86400))
//! forward : min(run_left, day_allow - used_today,        MONTH - calendar_month, MONTH - roll31)
//! gap     : min(run_left, day_allow - R(t) - used_today, MONTH - calendar_month, MONTH - ENR_WALL_MARGIN - roll31, paced)
//! enrich  : min(run_left, day_allow - R(t) - used_today, MONTH - calendar_month, MONTH - ENR_WALL_MARGIN - roll31, paced, enrich_paced)
//! paced        = day_allow * sec_of_day / 86400 + PACE_HEAD - used_today
//! enrich_allot = day_allow - fwd_need - ENRICH_SLACK
//! enrich_paced = enrich_allot * sec_of_day / 86400 + enrich_allot / 24 - enrich_used_today
//! ```
//!
//! Which term guards which window (keep all of them):
//! - `day_allow`: the workhorse; `31 * DAY_MAX < MONTH` makes every
//!   month-like window safe in steady state by construction.
//! - `MONTH - calendar_month`: the provider's unit if it bills by calendar
//!   month; independent code path (month prefix vs date arithmetic).
//! - `MONTH - roll31`: any calendar month, contract cycle or rolling
//!   30-day window is contained in a 31-day window. Fires only on
//!   misconfiguration.
//! - `ENR_WALL_MARGIN`: enrich stops earlier than forward, so a runaway
//!   starves enrich first and forward survives longer ("forward is the
//!   last thing to stop").
//!
//! The reserve `R(t)` has a proportional part (separates the two paced
//! lines during the day) and a floor (protects the last hours). Pacing
//! applies to the gap and enrich classes only: forward is bounded by its
//! own demand and `day_allow`.
//!
//! Float arithmetic deliberately mirrors the Python expressions (`as i64`
//! truncation == Python `int()` for these non-negative magnitudes) so the
//! golden fixture matches bit for bit.

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use std::collections::BTreeMap;

/// Per-plan limits. Only the Standard plan is supported; the values are
/// deliberately constants (not configuration) so nothing can raise the
/// monthly ceiling at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanLimits {
    /// Daily request allowance the policy enforces (set by the month:
    /// `31 * day_max` must clear the enrich wall).
    pub day_max: u64,
    /// Monthly ceiling the policy enforces (below the provider's 3.75M).
    pub month: u64,
    /// Requests one cycle may spend.
    pub run: u64,
    /// Head start of the intraday pacing line.
    pub pace_head: u64,
    /// Daily forward demand the reserve protects by default.
    pub need: u64,
    /// Floor of the reserve.
    pub reserve_min: u64,
    /// Enrich-class passes stop this many requests before the month wall.
    pub enr_wall_margin: u64,
    /// Enrich workers.
    pub workers: u32,
    /// Global request rate limits.
    pub rate_per_min: u32,
    pub rate_per_sec: u32,
    /// The provider's own hard daily limit (for assertions only).
    pub provider_day_hard: u64,
}

/// NearBlocks Standard (2026-09-19): provider 500/min, 187,500/day,
/// 3,750,000/month. `day_max` 113k is as close to 3.6M/31 as the enrich
/// wall allows.
pub const STANDARD: PlanLimits = PlanLimits {
    day_max: 113_000,
    month: 3_600_000,
    run: 6_000,
    pace_head: 4_000,
    need: 20_000,
    reserve_min: 2_500,
    enr_wall_margin: 40_000,
    workers: 4,
    rate_per_min: 420,
    rate_per_sec: 5,
    provider_day_hard: 187_500,
};

/// Days in the rolling quota window.
pub const ROLL_DAYS: u32 = 31;
/// Never prune the ledger below this many days or the window under-counts.
pub const LEDGER_MIN_KEEP_DAYS: u32 = 40;
/// Requests per day left unallocated between forward need and enrich share.
pub const ENRICH_SLACK: u64 = 600;
/// Never assume forward needs less than this per day.
pub const FWD_NEED_FLOOR: u64 = 4_000;
/// Margin over the 7-day mean forward spend.
pub const FWD_NEED_FACTOR: f64 = 1.25;
/// Margin over the last full day (reacts to a growth spurt within a day).
pub const FWD_NEED_LAST_FACTOR: f64 = 1.15;
/// A cycle must leave the next cron slot free.
pub const RUN_DEADLINE_S: u64 = 25 * 60;
/// Recent-gap catch-up may use at most this much of a cycle.
pub const GAP_TIME_S: u64 = 10 * 60;

const SECONDS_PER_DAY: u64 = 86_400;

// Invariants that must hold for the policy to be safe. Checked at compile
// time so a careless edit of a constant cannot ship.
const _: () = {
    assert!(STANDARD.day_max <= STANDARD.provider_day_hard);
    assert!(
        (ROLL_DAYS as u64) * STANDARD.day_max < STANDARD.month - STANDARD.enr_wall_margin,
        "31 * DAY_MAX must clear the enrich wall"
    );
    assert!(STANDARD.day_max > STANDARD.need && STANDARD.need > STANDARD.reserve_min);
    assert!(STANDARD.reserve_min > 0);
    assert!(STANDARD.enr_wall_margin > 0 && STANDARD.enr_wall_margin * 10 < STANDARD.month);
};

/// Budget class of a pass. Forward is the live list; gap is catch-up of
/// holes (ranks above enrich but pays the forward reserve and day pacing);
/// enrich is the detail fetches, additionally paced on its own allotment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stream {
    Forward,
    Gap,
    Enrich,
}

/// Requests charged per UTC day.
pub type Ledger = BTreeMap<NaiveDate, u64>;

/// Sum of the last `days` UTC days including `today`. Feeds the walls (a
/// bound on total consumption), not the daily allocator.
pub fn roll_sum(ledger: &Ledger, today: NaiveDate, days: u32) -> u64 {
    let start = today - chrono::TimeDelta::days(i64::from(days) - 1);
    ledger.range(start..=today).map(|(_, v)| *v).sum()
}

/// Requests charged in `today`'s calendar month (UTC).
pub fn used_calendar_month(ledger: &Ledger, today: NaiveDate) -> u64 {
    ledger
        .iter()
        .filter(|(day, _)| day.year() == today.year() && day.month() == today.month())
        .map(|(_, v)| *v)
        .sum()
}

/// Requests charged on `today`.
pub fn used_today(ledger: &Ledger, today: NaiveDate) -> u64 {
    ledger.get(&today).copied().unwrap_or(0)
}

fn sec_of_day(now: NaiveDateTime) -> u64 {
    u64::from(now.num_seconds_from_midnight())
}

/// Daily allowance: the plan's `day_max`, lowered (never raised) by the
/// operator cap.
pub fn day_allowance(plan: &PlanLimits, operator_cap: Option<u64>) -> u64 {
    match operator_cap {
        Some(cap) => cap.min(plan.day_max),
        None => plan.day_max,
    }
}

/// Daily requests set aside for forward when sizing the enrich share and
/// the reserve: the larger of `1.25 x` the 7-day mean and `1.15 x` the
/// last full day, floored at [`FWD_NEED_FLOOR`].
pub fn forward_need(fwd_7d_mean: u64, fwd_last_day: u64) -> u64 {
    FWD_NEED_FLOOR
        .max((fwd_7d_mean as f64 * FWD_NEED_FACTOR) as u64)
        .max((fwd_last_day as f64 * FWD_NEED_LAST_FACTOR) as u64)
}

/// Enrich's own daily share.
pub fn enrich_allotment(day_allow: u64, fwd_need: u64) -> u64 {
    day_allow
        .saturating_sub(fwd_need)
        .saturating_sub(ENRICH_SLACK)
}

/// Requests kept back from enrich-class passes for the rest of the UTC
/// day. The daily need is the larger of the plan constant and the
/// measured forward demand, capped at `0.9 x day_allow`.
pub fn reserve(
    plan: &PlanLimits,
    now: NaiveDateTime,
    day_allow: u64,
    fwd_need: Option<u64>,
) -> u64 {
    let remaining = SECONDS_PER_DAY - sec_of_day(now);
    let need = (0.9 * day_allow as f64).min(plan.need.max(fwd_need.unwrap_or(0)) as f64);
    let prop = (need * remaining as f64 / SECONDS_PER_DAY as f64) as u64;
    plan.reserve_min.max(prop)
}

/// Everything [`compute_budget`] needs; all values are the caller's view
/// of "now".
#[derive(Debug, Clone)]
pub struct BudgetInput<'a> {
    pub ledger: &'a Ledger,
    pub plan: &'a PlanLimits,
    pub now: NaiveDateTime,
    /// Requests already spent in this cycle.
    pub calls_this_run: u64,
    pub stream: Stream,
    /// Cycle cap (normally `plan.run`).
    pub budget_run: u64,
    /// Disable intraday pacing (manual catch-up).
    pub no_pace: bool,
    pub rate_limited: bool,
    pub net_down: bool,
    /// Measured forward demand; see [`forward_need`].
    pub fwd_need: Option<u64>,
    /// Enrich-class requests already spent today.
    pub enrich_used_today: Option<u64>,
    /// Operator cap on the daily allowance; see [`day_allowance`].
    pub operator_cap: Option<u64>,
}

/// Requests the caller may still spend right now. Never negative.
pub fn compute_budget(input: &BudgetInput<'_>) -> u64 {
    if input.rate_limited || input.net_down {
        return 0;
    }
    let plan = input.plan;
    let today = input.now.date();
    let day_allow = day_allowance(plan, input.operator_cap);
    let used_day = used_today(input.ledger, today);
    let r31 = roll_sum(input.ledger, today, ROLL_DAYS);
    let month_used = used_calendar_month(input.ledger, today);
    let (day_cap, wall) = match input.stream {
        Stream::Forward => (day_allow as i64, plan.month as i64),
        Stream::Gap | Stream::Enrich => (
            day_allow as i64 - reserve(plan, input.now, day_allow, input.fwd_need) as i64,
            plan.month as i64 - plan.enr_wall_margin as i64,
        ),
    };
    let mut left = (input.budget_run as i64 - input.calls_this_run as i64)
        .min(day_cap - used_day as i64)
        .min(plan.month as i64 - month_used as i64)
        .min(wall - r31 as i64);
    if !input.no_pace && input.stream != Stream::Forward {
        let sec = sec_of_day(input.now) as f64;
        let paced = (day_allow as f64 * sec / SECONDS_PER_DAY as f64) as i64
            + plan.pace_head as i64
            - used_day as i64;
        left = left.min(paced);
        if input.stream == Stream::Enrich
            && let (Some(fwd_need), Some(enrich_used)) = (input.fwd_need, input.enrich_used_today)
        {
            let allot = enrich_allotment(day_allow, fwd_need);
            let e_paced = (allot as f64 * sec / SECONDS_PER_DAY as f64) as i64
                + (allot / 24) as i64
                - enrich_used as i64;
            left = left.min(e_paced);
        }
    }
    left.max(0) as u64
}

/// Observability snapshot for the cycle-start log and the watchdog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub day_allow: u64,
    pub used_today: u64,
    pub roll31: u64,
    pub month_used: u64,
    pub month_cap: u64,
    pub enrich_wall: u64,
    pub reserve_now: u64,
    pub enrich_allot: Option<u64>,
    pub enrich_used: Option<u64>,
}

pub fn report(
    ledger: &Ledger,
    plan: &PlanLimits,
    now: NaiveDateTime,
    fwd_need: Option<u64>,
    enrich_used_today: Option<u64>,
    operator_cap: Option<u64>,
) -> Report {
    let today = now.date();
    let day_allow = day_allowance(plan, operator_cap);
    Report {
        day_allow,
        used_today: used_today(ledger, today),
        roll31: roll_sum(ledger, today, ROLL_DAYS),
        month_used: used_calendar_month(ledger, today),
        month_cap: plan.month,
        enrich_wall: plan.month - plan.enr_wall_margin,
        reserve_now: reserve(plan, now, day_allow, fwd_need),
        enrich_allot: fwd_need.map(|n| enrich_allotment(day_allow, n)),
        enrich_used: enrich_used_today,
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "budget: day_allow={} used_today={} roll31={}/{} (enrich wall {}) month={}/{} reserve_now={}",
            self.day_allow,
            self.used_today,
            self.roll31,
            self.month_cap,
            self.enrich_wall,
            self.month_used,
            self.month_cap,
            self.reserve_now,
        )?;
        if let Some(allot) = self.enrich_allot {
            write!(f, " enrich={}/{}", self.enrich_used.unwrap_or(0), allot)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
