use super::*;

pub fn date(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

pub fn at(y: i32, m: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
    date(y, m, d).and_hms_opt(h, mi, s).unwrap()
}

/// Ledger with consecutive days from `start`.
pub fn ledger_days(start: NaiveDate, values: &[u64]) -> Ledger {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (start + chrono::TimeDelta::days(i as i64), *v))
        .collect()
}

pub fn single(day: NaiveDate, used: u64) -> Ledger {
    Ledger::from([(day, used)])
}

/// Budget with the Standard plan and the given knobs.
pub fn bud(ledger: &Ledger, now: NaiveDateTime, stream: Stream, no_pace: bool) -> u64 {
    bud_with(ledger, now, stream, no_pace, None, None, 0)
}

pub fn bud_with(
    ledger: &Ledger,
    now: NaiveDateTime,
    stream: Stream,
    no_pace: bool,
    fwd_need: Option<u64>,
    enrich_used_today: Option<u64>,
    calls: u64,
) -> u64 {
    compute_budget(&BudgetInput {
        ledger,
        plan: &STANDARD,
        now,
        calls_this_run: calls,
        stream,
        budget_run: STANDARD.run,
        no_pace,
        rate_limited: false,
        net_down: false,
        fwd_need,
        enrich_used_today,
        operator_cap: None,
    })
}
