//! Waiting primitive shared by the cron-style loops in the workspace.
//!
//! Every periodic job (trade, DB maintenance, ref_flow collector) iterates
//! a `cron::Schedule` and must wait for the next tick without one long
//! `sleep`: the wall clock can jump (machine suspend, clock sync) and a
//! single long sleep would then fire late. [`sleep_until`] sleeps in
//! bounded slices and re-reads the wall clock between them.

use chrono::{DateTime, TimeZone, Utc};
use std::time::Duration;

/// Sleep until the wall clock reaches `next`, in slices of at most
/// `max_sleep`. Returns immediately when `next` is not in the future.
///
/// `on_slice` is called before every slice with the remaining duration,
/// so callers can log long waits without the helper knowing about their
/// logger or thresholds.
pub async fn sleep_until<Tz: TimeZone>(
    next: &DateTime<Tz>,
    max_sleep: Duration,
    mut on_slice: impl FnMut(Duration),
) {
    let next = next.with_timezone(&Utc);
    loop {
        let remaining = match (next - Utc::now()).to_std() {
            Ok(d) if !d.is_zero() => d,
            _ => return,
        };
        on_slice(remaining);
        tokio::time::sleep(remaining.min(max_sleep)).await;
    }
}

#[cfg(test)]
mod tests;
