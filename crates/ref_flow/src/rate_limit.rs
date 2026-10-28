//! Global send limiter shared by every worker: at most `per_minute` sends
//! in any 60-second window (the provider's per-minute cap) and at least
//! `min_interval` between consecutive sends (the provider's burst cap,
//! measured at ~10 req/s). Uses `tokio::time::Instant` so tests can drive
//! it with `tokio::time::pause`.

use std::collections::VecDeque;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};

const WINDOW: Duration = Duration::from_secs(60);
/// Slack added when waiting for the oldest send to leave the window.
const WINDOW_SLACK: Duration = Duration::from_millis(50);

#[derive(Debug)]
struct State {
    sends: VecDeque<Instant>,
    last_send: Option<Instant>,
}

#[derive(Debug)]
pub struct RateLimiter {
    per_minute: usize,
    min_interval: Duration,
    state: Mutex<State>,
}

impl RateLimiter {
    pub fn new(per_minute: u32, min_interval: Duration) -> Self {
        Self {
            per_minute: usize::try_from(per_minute).unwrap_or(usize::MAX),
            min_interval,
            state: Mutex::new(State {
                sends: VecDeque::new(),
                last_send: None,
            }),
        }
    }

    /// Limiter for the plan's `rate_per_min` and `rate_per_sec`.
    pub fn for_plan(plan: &crate::budget::PlanLimits) -> Self {
        let min_interval = Duration::from_secs_f64(1.0 / f64::from(plan.rate_per_sec.max(1)));
        Self::new(plan.rate_per_min, min_interval)
    }

    /// Wait until a send is allowed, then record it. The lock is only held
    /// to inspect and update the bookkeeping, never while sleeping.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut st = self.state.lock().await;
                let now = Instant::now();
                while st
                    .sends
                    .front()
                    .is_some_and(|&t| now.duration_since(t) > WINDOW)
                {
                    st.sends.pop_front();
                }
                let interval_wait = st
                    .last_send
                    .map(|t| (t + self.min_interval).saturating_duration_since(now))
                    .unwrap_or(Duration::ZERO);
                let window_wait = if st.sends.len() >= self.per_minute {
                    st.sends
                        .front()
                        .map(|&t| (t + WINDOW + WINDOW_SLACK).saturating_duration_since(now))
                        .unwrap_or(Duration::ZERO)
                } else {
                    Duration::ZERO
                };
                let wait = interval_wait.max(window_wait);
                if wait.is_zero() {
                    st.sends.push_back(now);
                    st.last_send = Some(now);
                    return;
                }
                wait
            };
            sleep(wait.max(Duration::from_millis(20))).await;
        }
    }
}

#[cfg(test)]
mod tests;
