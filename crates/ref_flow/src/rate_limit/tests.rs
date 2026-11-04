use super::*;

#[tokio::test(start_paused = true)]
async fn enforces_the_minimum_interval_between_sends() {
    let limiter = RateLimiter::new(1_000, Duration::from_millis(200));
    let start = Instant::now();
    limiter.acquire().await;
    assert_eq!(
        Instant::now() - start,
        Duration::ZERO,
        "first send is immediate"
    );
    limiter.acquire().await;
    assert!(Instant::now() - start >= Duration::from_millis(200));
    limiter.acquire().await;
    assert!(Instant::now() - start >= Duration::from_millis(400));
}

#[tokio::test(start_paused = true)]
async fn caps_sends_per_minute_window() {
    let limiter = RateLimiter::new(3, Duration::ZERO);
    let start = Instant::now();
    for _ in 0..3 {
        limiter.acquire().await;
    }
    assert_eq!(
        Instant::now() - start,
        Duration::ZERO,
        "three sends fit the window"
    );
    // The fourth must wait until the first leaves the 60 s window.
    limiter.acquire().await;
    let elapsed = Instant::now() - start;
    assert!(elapsed > Duration::from_secs(60), "waited {elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(61),
        "did not over-wait: {elapsed:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn window_slides_rather_than_resets() {
    let limiter = RateLimiter::new(2, Duration::ZERO);
    let start = Instant::now();
    limiter.acquire().await; // t=0
    tokio::time::advance(Duration::from_secs(30)).await;
    limiter.acquire().await; // t=30
    limiter.acquire().await; // must wait until t=60+
    let elapsed = Instant::now() - start;
    assert!(
        elapsed >= Duration::from_secs(60) && elapsed < Duration::from_secs(61),
        "{elapsed:?}"
    );
    limiter.acquire().await; // next free slot at t=90+
    let elapsed = Instant::now() - start;
    assert!(
        elapsed >= Duration::from_secs(90) && elapsed < Duration::from_secs(91),
        "{elapsed:?}"
    );
}

#[test]
fn plan_limiter_uses_the_plan_rates() {
    let l = RateLimiter::for_plan(&crate::budget::STANDARD);
    assert_eq!(l.per_minute, 420);
    assert_eq!(l.min_interval, Duration::from_millis(200));
}
