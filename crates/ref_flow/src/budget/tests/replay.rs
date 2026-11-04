//! Replay: a greedy collector consuming every cycle for hundreds of days.
//! Asserts containment properties of the policy, not specific numbers.

use super::*;

const RUNS_PER_DAY: u64 = 48;
/// ~10k/day forward, the storm-month demand measured in September 2026.
const FWD_PER_RUN: u64 = 210;

struct Outcome {
    ledger: Ledger,
    min_forward_budget: u64,
    forward_starved_runs: u64,
}

fn simulate(
    start: NaiveDate,
    legacy: &Ledger,
    days: u32,
    fwd_per_run: u64,
    runaway_day: Option<u32>,
) -> Outcome {
    let mut ledger = legacy.clone();
    let mut min_forward_budget = u64::MAX;
    let mut forward_starved_runs = 0;
    for d in 0..days {
        for r in 0..RUNS_PER_DAY {
            let now = start.and_hms_opt(0, 0, 0).unwrap()
                + chrono::TimeDelta::days(i64::from(d))
                + chrono::TimeDelta::seconds((r * 1800 + 5) as i64);
            let day = now.date();
            if runaway_day == Some(d) {
                // Misconfiguration: someone spends 500 per run ignoring the
                // daily allowance but not the walls.
                let f = compute_budget(&BudgetInput {
                    ledger: &ledger,
                    plan: &STANDARD,
                    now,
                    calls_this_run: 0,
                    stream: Stream::Forward,
                    budget_run: 1_000_000,
                    no_pace: true,
                    rate_limited: false,
                    net_down: false,
                    fwd_need: None,
                    enrich_used_today: None,
                    operator_cap: Some(1_000_000),
                })
                .min(500);
                *ledger.entry(day).or_insert(0) += f;
                continue;
            }
            let f_avail = bud(&ledger, now, Stream::Forward, true);
            let f = fwd_per_run.min(f_avail);
            min_forward_budget = min_forward_budget.min(f_avail);
            if f_avail < fwd_per_run {
                forward_starved_runs += 1;
            }
            *ledger.entry(day).or_insert(0) += f;
            let e = bud_with(&ledger, now, Stream::Enrich, true, None, None, f);
            *ledger.entry(day).or_insert(0) += e;
        }
    }
    Outcome {
        ledger,
        min_forward_budget,
        forward_starved_runs,
    }
}

fn max_window(ledger: &Ledger, days: i64) -> u64 {
    let (first, last) = (
        *ledger.keys().next().unwrap(),
        *ledger.keys().next_back().unwrap(),
    );
    let mut best = 0;
    let mut cur = first;
    while cur <= last {
        let end = cur + chrono::TimeDelta::days(days - 1);
        let sum: u64 = ledger.range(cur..=end).map(|(_, v)| *v).sum();
        best = best.max(sum);
        cur += chrono::TimeDelta::days(1);
    }
    best
}

/// Snapshot shaped like the real ledger right after the Standard switch
/// (2026-09-19..10-25): a few heavy days, then 110.5k/day steady state.
fn legacy() -> (Ledger, NaiveDate) {
    let start = date(2026, 9, 19);
    let vals = [
        62_445, 85_602, 71_789, 62_879, 78_513, 52_114, 40_622, 103_774, 104_773, 106_084, 110_904,
        110_680, 110_618, 110_665, 110_740, 110_776, 110_972, 110_964, 110_996, 110_664, 103_480,
        46_704, 32_031, 43_623, 110_547, 110_500, 110_529, 110_518, 110_500, 110_500, 110_500,
        110_510, 110_500, 110_500, 110_526, 110_500,
    ];
    (ledger_days(start, &vals), date(2026, 10, 26))
}

#[test]
fn all_windows_stay_below_the_month_cap_and_forward_is_never_starved() {
    let (legacy, start) = legacy();
    let out = simulate(start, &legacy, 400, FWD_PER_RUN, None);
    assert!(max_window(&out.ledger, 31) < STANDARD.month);
    assert!(max_window(&out.ledger, 30) < STANDARD.month);
    let mut months: BTreeMap<(i32, u32), u64> = BTreeMap::new();
    for (day, v) in &out.ledger {
        *months.entry((day.year(), day.month())).or_insert(0) += v;
    }
    assert!(months.values().all(|&v| v < STANDARD.month));
    assert_eq!(out.forward_starved_runs, 0);
    assert!(out.min_forward_budget > 0);
    // The greedy collector actually uses the allowance (not trivially safe).
    let steady: u64 = out
        .ledger
        .range(date(2027, 1, 1)..=date(2027, 1, 31))
        .map(|(_, v)| *v)
        .sum();
    assert!(steady > 31 * 100_000, "january total {steady}");
}

#[test]
fn forward_survives_1_5x_measured_demand() {
    let (legacy, start) = legacy();
    let out = simulate(start, &legacy, 120, FWD_PER_RUN * 3 / 2, None);
    assert_eq!(out.forward_starved_runs, 0);
}

#[test]
fn demand_above_need_starves_forward_late_in_the_day() {
    // Documents the design envelope: NEED (20k/day) is the forward demand
    // the reserve protects; beyond it forward loses the last run(s) of the
    // day, which is the signal to re-derive NEED.
    let (legacy, start) = legacy();
    let out = simulate(start, &legacy, 30, 520, None); // ~25k/day > NEED
    assert!(out.forward_starved_runs > 0);
}

#[test]
fn runaway_day_is_contained_by_the_walls() {
    let (legacy, start) = legacy();
    let out = simulate(start, &legacy, 80, FWD_PER_RUN, Some(40));
    assert!(max_window(&out.ledger, 31) <= STANDARD.month + 500);
}
