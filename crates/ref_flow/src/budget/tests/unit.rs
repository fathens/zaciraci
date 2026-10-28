//! Behavioural tests ported from the Python `test_budget.py`, on the
//! Standard plan, plus the operator cap that only exists in the port.

use super::*;

const P: &PlanLimits = &STANDARD;

#[test]
fn constants_relationships() {
    assert!(u64::from(ROLL_DAYS) * P.day_max < P.month - P.enr_wall_margin);
    assert!(P.day_max > P.need && P.need > P.reserve_min);
    assert!(P.day_max <= P.provider_day_hard);
    assert!(P.month < 3_750_000, "stay below the provider's monthly cap");
    // Enrich share at the measured forward demand leaves room for full
    // enrichment (~94k successful txns/day on a storm day).
    let need = forward_need(11_000, 18_000);
    assert!(enrich_allotment(P.day_max, need) > 85_000);
}

#[test]
fn operator_cap_only_lowers_the_allowance() {
    assert_eq!(day_allowance(P, None), P.day_max);
    assert_eq!(day_allowance(P, Some(50_000)), 50_000);
    assert_eq!(day_allowance(P, Some(P.day_max)), P.day_max);
    assert_eq!(day_allowance(P, Some(10_000_000)), P.day_max);
    assert_eq!(day_allowance(P, Some(0)), 0);

    let now = at(2026, 10, 26, 12, 0, 0);
    let ledger = single(now.date(), 40_000);
    let capped = compute_budget(&BudgetInput {
        ledger: &ledger,
        plan: P,
        now,
        calls_this_run: 0,
        stream: Stream::Forward,
        budget_run: P.run,
        no_pace: true,
        rate_limited: false,
        net_down: false,
        fwd_need: None,
        enrich_used_today: None,
        operator_cap: Some(45_000),
    });
    assert_eq!(capped, 5_000, "cap 45k with 40k used leaves 5k");
    let reported = report(&ledger, P, now, Some(20_000), Some(0), Some(45_000));
    assert_eq!(reported.day_allow, 45_000);
    assert_eq!(
        reported.enrich_allot,
        Some(enrich_allotment(45_000, 20_000))
    );
}

#[test]
fn reserve_is_proportional_then_floored() {
    let da = P.day_max;
    let start = reserve(P, at(2026, 10, 10, 0, 0, 0), da, None);
    assert_eq!(start, P.need, "full NEED at 00:00");
    assert_eq!(
        reserve(P, at(2026, 10, 10, 23, 59, 59), da, None),
        P.reserve_min
    );
    let mut prev = start;
    for h in 0..24 {
        let r = reserve(P, at(2026, 10, 10, h, 0, 0), da, None);
        assert!(r <= prev, "monotone non-increasing at {h}h");
        prev = r;
    }
}

#[test]
fn reserve_follows_measured_forward_demand() {
    let da = P.day_max;
    let now = at(2026, 10, 18, 0, 0, 0);
    let base = reserve(P, now, da, None);
    assert_eq!(base, P.need);
    assert_eq!(
        reserve(P, now, da, Some(P.need - 1000)),
        base,
        "never below the constant"
    );
    assert_eq!(reserve(P, now, da, Some(P.need + 1000)), P.need + 1000);
    assert_eq!(
        reserve(P, now, da, Some(1_000_000)),
        (0.9 * da as f64) as u64,
        "capped"
    );
    assert!(reserve(P, now, da, None) < da);

    // Enrich budget shrinks accordingly; forward does not care.
    let led = single(now.date(), da - (P.need + 1000) - 300);
    let e_lo = bud_with(&led, now, Stream::Enrich, true, None, None, 0);
    let e_hi = bud_with(
        &led,
        now,
        Stream::Enrich,
        true,
        Some(P.need + 1000),
        None,
        0,
    );
    assert_eq!(e_hi, 300);
    assert_eq!(e_lo, P.run.min(1300));
    assert_eq!(
        bud_with(&led, now, Stream::Forward, true, Some(1_000_000), None, 0),
        P.run.min(P.need + 1300)
    );
}

#[test]
fn ledger_aggregates_include_today_and_ignore_other_months() {
    let today = date(2026, 10, 10);
    let led = ledger_days(today - chrono::TimeDelta::days(40), &[100; 41]);
    let now = today.and_hms_opt(12, 0, 0).unwrap();
    assert_eq!(roll_sum(&led, now.date(), 31), 3100);
    assert_eq!(used_today(&led, now.date()), 100);
    assert_eq!(used_calendar_month(&led, now.date()), 1000, "10-01..10-10");
    assert_eq!(roll_sum(&Ledger::new(), now.date(), 31), 0);
    // Future days never count towards the window.
    let mut with_future = led.clone();
    with_future.insert(date(2027, 1, 1), 999);
    assert_eq!(roll_sum(&with_future, now.date(), 31), 3100);
}

#[test]
fn blocked_states_and_bounds() {
    let now = at(2026, 10, 10, 12, 0, 0);
    let empty = Ledger::new();
    assert_eq!(bud(&empty, now, Stream::Forward, true), P.run);
    assert!(bud(&empty, now, Stream::Forward, false) <= P.run);
    for stream in [Stream::Forward, Stream::Gap, Stream::Enrich] {
        let blocked = BudgetInput {
            ledger: &empty,
            plan: P,
            now,
            calls_this_run: 0,
            stream,
            budget_run: P.run,
            no_pace: false,
            rate_limited: true,
            net_down: false,
            fwd_need: None,
            enrich_used_today: None,
            operator_cap: None,
        };
        assert_eq!(compute_budget(&blocked), 0);
        assert_eq!(
            compute_budget(&BudgetInput {
                rate_limited: false,
                net_down: true,
                ..blocked
            }),
            0
        );
        let exhausted = single(now.date(), 1_000_000);
        assert_eq!(bud(&exhausted, now, stream, false), 0, "never negative");
        assert!(bud(&empty, now, stream, true) <= P.run);
    }
    assert_eq!(
        bud_with(&empty, now, Stream::Forward, true, None, None, P.run),
        0
    );
}

#[test]
fn forward_keeps_going_when_enrich_is_reserved_out() {
    let da = P.day_max;
    let now = at(2026, 10, 10, 22, 0, 0);
    let r = reserve(P, now, da, None);
    let led = single(now.date(), da - r);
    assert_eq!(bud(&led, now, Stream::Enrich, true), 0);
    assert_eq!(bud(&led, now, Stream::Forward, true), r);
}

#[test]
fn used_today_is_counted_once() {
    let now = at(2026, 10, 10, 12, 0, 0);
    let da = P.day_max;
    let r = reserve(P, now, da, None);
    let f1 = bud(&single(now.date(), da - 1000), now, Stream::Forward, true);
    let f2 = bud(&single(now.date(), da - 500), now, Stream::Forward, true);
    assert_eq!((f1, f2), (1000, 500));
    let e1 = bud(
        &single(now.date(), da - r - 1000),
        now,
        Stream::Enrich,
        true,
    );
    let e2 = bud(&single(now.date(), da - r - 500), now, Stream::Enrich, true);
    assert_eq!((e1, e2), (1000, 500));
}

#[test]
fn day_boundary_uses_now_not_a_cached_day() {
    let day1 = at(2026, 10, 10, 23, 59, 59);
    let day2 = day1 + chrono::TimeDelta::seconds(1);
    let led = single(day1.date(), P.day_max);
    assert_eq!(bud(&led, day1, Stream::Forward, true), 0);
    assert!(bud(&led, day2, Stream::Forward, true) > 0);
}

#[test]
fn pacing_cannot_exceed_day_cap_and_applies_to_gap_and_enrich_only() {
    let now = at(2026, 10, 10, 23, 0, 0);
    let led = single(now.date(), P.day_max - 10);
    assert!(bud(&led, now, Stream::Enrich, false) <= 10);
    assert!(bud(&led, now, Stream::Forward, false) <= 10);

    // Early in the day with a big spend already booked: gap/enrich are paced
    // to 0, forward still sees the remaining allowance (bounded by run cap).
    let early = at(2026, 10, 10, 1, 0, 0);
    let led = single(early.date(), 60_000);
    assert_eq!(bud(&led, early, Stream::Enrich, false), 0);
    assert_eq!(bud(&led, early, Stream::Gap, false), 0);
    assert_eq!(
        bud(&led, early, Stream::Forward, false),
        P.run.min(P.day_max - 60_000)
    );
}

#[test]
fn walls_stop_streams_in_order() {
    let today = date(2026, 10, 20);
    let now = today.and_hms_opt(6, 0, 0).unwrap();
    let wall_e = P.month - P.enr_wall_margin;
    let mut led = ledger_days(today - chrono::TimeDelta::days(30), &[wall_e / 30; 30]);
    led.insert(today, 0);
    let r31 = roll_sum(&led, today, ROLL_DAYS);
    assert!(bud(&led, now, Stream::Forward, true) >= P.run.min(P.month - r31) - 1);
    assert!(bud(&led, now, Stream::Enrich, true) <= wall_e.saturating_sub(r31));
    // Push past the forward wall too.
    led.insert(today, P.month - r31 + 1);
    assert_eq!(bud(&led, now, Stream::Forward, true), 0);

    // Calendar-month guard is an independent path.
    let now = at(2026, 10, 10, 12, 0, 0);
    let led = single(date(2026, 10, 1), P.month);
    assert_eq!(bud(&led, now, Stream::Forward, true), 0);
    assert_eq!(bud(&led, now, Stream::Enrich, true), 0);
}

#[test]
fn enrich_is_paced_on_its_own_share_forward_unaffected() {
    let da = P.day_max;
    let need = forward_need(11_000, 0);
    let allot = enrich_allotment(da, need);
    assert_eq!(allot, da - need - ENRICH_SLACK);
    let now = at(2026, 10, 10, 12, 0, 0);
    let led = single(now.date(), 6_000);
    let e = bud_with(&led, now, Stream::Enrich, false, Some(need), Some(0), 0);
    assert!(e <= allot / 2 + allot / 24);
    assert!(e > 0);
    let e2 = bud_with(
        &led,
        now,
        Stream::Enrich,
        false,
        Some(need),
        Some(allot / 2 + allot / 24),
        0,
    );
    assert_eq!(e2, 0);
    assert_eq!(
        bud_with(
            &led,
            now,
            Stream::Forward,
            true,
            Some(need),
            Some(1_000_000),
            0
        ),
        P.run.min(da - 6_000)
    );
}

#[test]
fn allotment_and_forward_need_edge_cases() {
    assert_eq!(enrich_allotment(12_000, 20_000), 0);
    assert_eq!(forward_need(0, 0), FWD_NEED_FLOOR);
    assert_eq!(
        forward_need(9_000, 8_000),
        (9_000.0 * FWD_NEED_FACTOR) as u64
    );
    assert_eq!(
        forward_need(9_000, 11_448),
        (11_448.0 * FWD_NEED_LAST_FACTOR) as u64
    );
}

#[test]
fn gap_pays_reserve_and_pacing_but_not_enrich_allotment() {
    let da = P.day_max;
    let need = forward_need(11_000, 0);
    let now = at(2026, 10, 12, 3, 0, 0);
    let led = single(now.date(), 2_000);
    let g = bud_with(
        &led,
        now,
        Stream::Gap,
        false,
        Some(need),
        Some(1_000_000),
        0,
    );
    let e = bud_with(
        &led,
        now,
        Stream::Enrich,
        false,
        Some(need),
        Some(1_000_000),
        0,
    );
    let f = bud_with(
        &led,
        now,
        Stream::Forward,
        true,
        Some(need),
        Some(1_000_000),
        0,
    );
    assert_eq!(e, 0, "enrich blocked by its allotment");
    assert!(g > 0, "gap still allowed");
    assert!(
        g <= (da as f64 * 3.0 / 24.0) as u64 + P.pace_head - 2_000,
        "but day-paced"
    );
    assert_eq!(f, P.run.min(da - 2_000));
    // Gap can never eat the forward reserve.
    let r = reserve(P, now, da, None);
    let led2 = single(now.date(), da - r);
    assert_eq!(bud(&led2, now, Stream::Gap, true), 0);
    assert_eq!(bud(&led2, now, Stream::Forward, true), P.run.min(r));
}

#[test]
fn report_renders_the_cycle_start_line() {
    let now = at(2026, 10, 26, 10, 0, 0);
    let led = single(now.date(), 1_234);
    let r = report(&led, P, now, Some(20_000), Some(500), None);
    let text = r.to_string();
    assert!(text.starts_with("budget: day_allow=113000 used_today=1234 roll31=1234/3600000"));
    assert!(text.contains("enrich=500/92400"), "{text}");
    let r = report(&led, P, now, None, None, None);
    assert!(!r.to_string().contains("enrich="));
}
