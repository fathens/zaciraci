use super::*;
use chrono::{FixedOffset, TimeDelta};
use std::time::Instant;

#[tokio::test]
async fn returns_immediately_when_next_is_not_in_the_future() {
    let mut slices = 0;
    let started = Instant::now();
    sleep_until(
        &(Utc::now() - TimeDelta::seconds(5)),
        Duration::from_secs(60),
        |_| slices += 1,
    )
    .await;
    assert_eq!(slices, 0);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[tokio::test]
async fn waits_in_bounded_slices_until_next() {
    let next = Utc::now() + TimeDelta::milliseconds(120);
    let mut remaining_seen = Vec::new();
    let started = Instant::now();
    sleep_until(&next, Duration::from_millis(30), |remaining| {
        remaining_seen.push(remaining)
    })
    .await;
    assert!(Utc::now() >= next, "must not return before next");
    assert!(
        started.elapsed() >= Duration::from_millis(120),
        "returned after {:?}",
        started.elapsed()
    );
    // 120 ms in 30 ms slices needs at least four slices, and the reported
    // remaining time must shrink monotonically.
    assert!(remaining_seen.len() >= 4, "slices: {remaining_seen:?}");
    assert!(
        remaining_seen.windows(2).all(|w| w[0] > w[1]),
        "remaining must decrease: {remaining_seen:?}"
    );
    assert!(remaining_seen[0] <= Duration::from_millis(120));
}

#[tokio::test]
async fn single_slice_when_max_sleep_exceeds_the_wait() {
    let next = Utc::now() + TimeDelta::milliseconds(40);
    let mut slices = 0;
    sleep_until(&next, Duration::from_secs(60), |_| slices += 1).await;
    assert!(Utc::now() >= next);
    // One slice covers the whole wait; a second iteration only happens if
    // the sleep woke up marginally early.
    assert!(slices <= 2, "slices: {slices}");
}

#[tokio::test]
async fn accepts_any_time_zone() {
    let jst = FixedOffset::east_opt(9 * 3600).unwrap();
    let next = (Utc::now() + TimeDelta::milliseconds(30)).with_timezone(&jst);
    sleep_until(&next, Duration::from_millis(10), |_| {}).await;
    assert!(Utc::now() >= next.with_timezone(&Utc));
}
