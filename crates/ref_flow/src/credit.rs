//! Request permits: the type-level guarantee that nothing is sent to
//! NearBlocks without having been charged to the ledger first.
//!
//! The collector charges the ledger in small blocks (a [`Reservation`])
//! and hands out one [`Credit`] per HTTP attempt. A `Credit` is not
//! `Clone`, so each attempt, including retries, costs exactly one; and
//! because a reservation is bound to a UTC day, a block charged yesterday
//! can never be spent today (the ledger stays an upper bound per day).

use chrono::NaiveDate;

/// Permission to send one request. Consumed by the client when it sends.
#[derive(Debug)]
#[must_use = "a credit that is dropped unused is still charged to the ledger"]
pub struct Credit {
    day: NaiveDate,
}

impl Credit {
    /// UTC day the credit was charged to.
    pub fn day(&self) -> NaiveDate {
        self.day
    }
}

/// A block of requests already charged to the ledger for one UTC day.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reservation {
    day: NaiveDate,
    remaining: u32,
}

impl Reservation {
    /// `requests` have been charged to `day`.
    pub fn new(day: NaiveDate, requests: u32) -> Self {
        Self {
            day,
            remaining: requests,
        }
    }

    pub fn day(&self) -> NaiveDate {
        self.day
    }

    pub fn remaining(&self) -> u32 {
        self.remaining
    }

    /// Take one credit if the reservation belongs to `today` and is not
    /// exhausted. A reservation from another day yields nothing, so its
    /// leftover is simply forfeited (already charged, never sent).
    /// Add `requests` to the block if it belongs to `today`; a block from
    /// another day is forfeited and replaced by a fresh one.
    pub fn add(&mut self, today: NaiveDate, requests: u32) -> u32 {
        if self.day != today {
            *self = Self::new(today, 0);
        }
        self.remaining = self.remaining.saturating_add(requests);
        self.remaining
    }

    pub fn take(&mut self, today: NaiveDate) -> Option<Credit> {
        if self.day != today || self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(Credit { day: self.day })
    }
}

/// Where the client obtains the credit for each attempt. Implemented by
/// the collector's shared, refillable reservation.
pub trait CreditSource: Send + Sync {
    fn take(&self, today: NaiveDate) -> Option<Credit>;
}

impl CreditSource for std::sync::Mutex<Reservation> {
    fn take(&self, today: NaiveDate) -> Option<Credit> {
        self.lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take(today)
    }
}

#[cfg(test)]
mod tests;
