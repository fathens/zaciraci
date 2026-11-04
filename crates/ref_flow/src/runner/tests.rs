use super::*;
use chrono::Timelike;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

fn row(d: u32, stream: &str, credits: i64) -> DailyCredits {
    DailyCredits::new(day(d), stream.parse().unwrap(), credits)
}

#[test]
fn stream_stats_average_the_seven_full_days_before_today() {
    let today = day(26);
    let mut rows = Vec::new();
    // 7 full days 19..25: 700 each = 4900 / 7 = 700.
    for d in 19..=25 {
        rows.push(row(d, "fwd", 700));
        rows.push(row(d, "enrich", 50_000));
    }
    // Today and an 8th day back are excluded from the mean.
    rows.push(row(26, "fwd", 10_000));
    rows.push(row(18, "fwd", 10_000));
    rows.push(row(26, "enrich", 1_234));
    rows.push(row(26, "gap", 999));
    let stats = stream_stats_from(&rows, today).unwrap();
    assert_eq!(stats.fwd_7d_mean, 700);
    assert_eq!(stats.fwd_last_day, 700);
    assert_eq!(
        stats.enrich_used_today, 1_234,
        "gap credits are not enrich spend"
    );
}

#[test]
fn stream_stats_tolerate_missing_days() {
    let stats = stream_stats_from(&[], day(26)).unwrap();
    assert_eq!(stats, StreamStats::default());
    let stats = stream_stats_from(&[row(25, "fwd", 1_400)], day(26)).unwrap();
    assert_eq!(stats.fwd_7d_mean, 200);
    assert_eq!(stats.fwd_last_day, 1_400);
}

#[test]
fn schedule_falls_back_to_the_default_on_parse_failure() {
    let log = DEFAULT.new(o!("test" => "schedule"));
    let bad = parse_schedule("every half hour please", &log);
    let good = parse_schedule(DEFAULT_CRON_SCHEDULE, &log);
    assert_eq!(
        bad.upcoming(Utc).next().unwrap(),
        good.upcoming(Utc).next().unwrap()
    );
    let custom = parse_schedule("0 15 * * * *", &log);
    assert_eq!(custom.upcoming(Utc).next().unwrap().minute(), 15);
}

#[test]
fn minimum_cycle_interval_is_a_hard_floor_below_any_sane_schedule() {
    assert!(MIN_CYCLE_INTERVAL >= Duration::from_secs(600));
    assert!(MIN_CYCLE_INTERVAL < Duration::from_secs(30 * 60));
}
