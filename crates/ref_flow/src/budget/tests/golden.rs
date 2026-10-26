//! Bit-for-bit comparison with the frozen Python policy.

use super::*;
use serde::Deserialize;

const GOLDEN: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/budget_golden.json"
));

#[derive(Deserialize)]
struct Golden {
    plan: String,
    limits: Limits,
    cases: Vec<Case>,
    forward_need_cases: Vec<ForwardNeedCase>,
    allotment_cases: Vec<AllotmentCase>,
}

#[derive(Deserialize)]
struct Limits {
    day_max: u64,
    month: u64,
    run: u64,
    pace_head: u64,
    need: u64,
    reserve_min: u64,
    enr_wall_margin: u64,
}

#[derive(Deserialize)]
struct Case {
    ledger: BTreeMap<String, u64>,
    now: String,
    calls: u64,
    stream: String,
    no_pace: bool,
    rate_limited: bool,
    net_down: bool,
    fwd_need: Option<u64>,
    enrich_used_today: Option<u64>,
    expect: Expect,
}

#[derive(Deserialize)]
struct Expect {
    budget: u64,
    reserve: u64,
    used_today: u64,
    roll31: u64,
    month_used: u64,
    day_allow: u64,
}

#[derive(Deserialize)]
struct ForwardNeedCase {
    mean: u64,
    last: u64,
    expect: u64,
}

#[derive(Deserialize)]
struct AllotmentCase {
    day_allow: u64,
    fwd_need: u64,
    expect: u64,
}

fn load() -> Golden {
    serde_json::from_str(GOLDEN).expect("budget_golden.json must parse")
}

fn stream(s: &str) -> Stream {
    match s {
        "forward" => Stream::Forward,
        "gap" => Stream::Gap,
        "enrich" => Stream::Enrich,
        other => panic!("unknown stream in fixture: {other}"),
    }
}

#[test]
fn fixture_targets_the_standard_plan_constants() {
    let g = load();
    assert_eq!(g.plan, "standard");
    assert_eq!(g.limits.day_max, STANDARD.day_max);
    assert_eq!(g.limits.month, STANDARD.month);
    assert_eq!(g.limits.run, STANDARD.run);
    assert_eq!(g.limits.pace_head, STANDARD.pace_head);
    assert_eq!(g.limits.need, STANDARD.need);
    assert_eq!(g.limits.reserve_min, STANDARD.reserve_min);
    assert_eq!(g.limits.enr_wall_margin, STANDARD.enr_wall_margin);
}

#[test]
fn compute_budget_matches_python_reference() {
    let g = load();
    assert!(g.cases.len() > 3000, "fixture has {} cases", g.cases.len());
    let mut by_stream = [0usize; 3];
    for (i, case) in g.cases.iter().enumerate() {
        let ledger: Ledger = case
            .ledger
            .iter()
            .map(|(k, v)| (NaiveDate::parse_from_str(k, "%Y-%m-%d").unwrap(), *v))
            .collect();
        let now = NaiveDateTime::parse_from_str(&case.now, "%Y-%m-%dT%H:%M:%S").unwrap();
        let st = stream(&case.stream);
        by_stream[st as usize] += 1;
        let input = BudgetInput {
            ledger: &ledger,
            plan: &STANDARD,
            now,
            calls_this_run: case.calls,
            stream: st,
            budget_run: STANDARD.run,
            no_pace: case.no_pace,
            rate_limited: case.rate_limited,
            net_down: case.net_down,
            fwd_need: case.fwd_need,
            enrich_used_today: case.enrich_used_today,
            operator_cap: None,
        };
        let today = now.date();
        assert_eq!(
            compute_budget(&input),
            case.expect.budget,
            "case {i}: budget ({})",
            case.now
        );
        assert_eq!(
            reserve(&STANDARD, now, STANDARD.day_max, case.fwd_need),
            case.expect.reserve,
            "case {i}: reserve"
        );
        assert_eq!(
            used_today(&ledger, today),
            case.expect.used_today,
            "case {i}: used_today"
        );
        assert_eq!(
            roll_sum(&ledger, today, ROLL_DAYS),
            case.expect.roll31,
            "case {i}: roll31"
        );
        assert_eq!(
            used_calendar_month(&ledger, today),
            case.expect.month_used,
            "case {i}: month_used"
        );
        assert_eq!(
            day_allowance(&STANDARD, None),
            case.expect.day_allow,
            "case {i}: day_allow"
        );
    }
    assert!(
        by_stream.iter().all(|&n| n > 500),
        "every stream covered: {by_stream:?}"
    );
}

#[test]
fn forward_need_and_allotment_match_python_reference() {
    let g = load();
    for c in &g.forward_need_cases {
        assert_eq!(
            forward_need(c.mean, c.last),
            c.expect,
            "forward_need({}, {})",
            c.mean,
            c.last
        );
    }
    for c in &g.allotment_cases {
        assert_eq!(
            enrich_allotment(c.day_allow, c.fwd_need),
            c.expect,
            "enrich_allotment({}, {})",
            c.day_allow,
            c.fwd_need
        );
    }
}
