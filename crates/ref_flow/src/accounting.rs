//! Per-cycle request accounting: ties the pure budget policy to the
//! ledger and hands out [`Reservation`] blocks to the client.
//!
//! The tracker keeps an in-memory copy of the ledger window, pre-charges
//! requests to the day ledger in small blocks (never more than the policy
//! allows for the current stream right now) and exposes the shared
//! reservation the client draws [`Credit`](crate::credit::Credit)s from.
//! A block charged on one UTC day is forfeited at midnight.

use crate::budget::{self, BudgetInput, Ledger, PlanLimits, Stream};
use crate::credit::Reservation;
use anyhow::Result;
use chrono::{NaiveDate, NaiveDateTime};
use std::future::Future;
use std::sync::{Arc, Mutex};

/// Requests pre-charged per block. Small enough that the forfeited
/// remainder at midnight or cycle end is negligible, large enough that
/// the four enrich workers rarely stall on a refill.
pub const RESERVE_BLOCK: u32 = 20;

/// Durable ledger behind the tracker.
pub trait LedgerStore: Send + Sync {
    /// Charge `requests` to `day`; returns the day's total afterwards.
    fn add_requests(
        &self,
        day: NaiveDate,
        requests: u32,
    ) -> impl Future<Output = Result<u64>> + Send;
}

/// The production store (`ref_flow_ledger`).
#[derive(Debug, Clone, Copy, Default)]
pub struct DbLedger;

impl LedgerStore for DbLedger {
    async fn add_requests(&self, day: NaiveDate, requests: u32) -> Result<u64> {
        persistence::ref_flow::ledger::add_requests(day, requests).await
    }
}

/// Forward demand and enrich spend observed so far, from `ref_flow_runs`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Mean forward credits per day over the last 7 full days.
    pub fwd_7d_mean: u64,
    /// Forward credits on the last full day.
    pub fwd_last_day: u64,
    /// Enrich-class credits already charged today before this cycle.
    pub enrich_used_today: u64,
}

#[derive(Debug)]
pub struct BudgetTracker<L: LedgerStore> {
    store: L,
    plan: &'static PlanLimits,
    ledger: Ledger,
    reservation: Arc<Mutex<Reservation>>,
    calls_this_run: u64,
    enrich_used_today: u64,
    fwd_need: u64,
    operator_cap: Option<u64>,
    rate_limited: bool,
    net_down: bool,
}

impl<L: LedgerStore> BudgetTracker<L> {
    /// `ledger` is the stored window (at least the last 40 days).
    pub fn new(
        store: L,
        plan: &'static PlanLimits,
        ledger: Ledger,
        stats: StreamStats,
        operator_cap: Option<u64>,
        today: NaiveDate,
    ) -> Self {
        Self {
            store,
            plan,
            ledger,
            reservation: Arc::new(Mutex::new(Reservation::new(today, 0))),
            calls_this_run: 0,
            enrich_used_today: stats.enrich_used_today,
            fwd_need: budget::forward_need(stats.fwd_7d_mean, stats.fwd_last_day),
            operator_cap,
            rate_limited: false,
            net_down: false,
        }
    }

    /// The reservation the HTTP client draws credits from.
    pub fn credits(&self) -> Arc<Mutex<Reservation>> {
        Arc::clone(&self.reservation)
    }

    pub fn calls_this_run(&self) -> u64 {
        self.calls_this_run
    }

    pub fn fwd_need(&self) -> u64 {
        self.fwd_need
    }

    pub fn set_rate_limited(&mut self) {
        self.rate_limited = true;
    }

    pub fn is_rate_limited(&self) -> bool {
        self.rate_limited
    }

    pub fn set_net_down(&mut self) {
        self.net_down = true;
    }

    pub fn is_net_down(&self) -> bool {
        self.net_down
    }

    /// Requests the policy allows `stream` to spend right now, on top of
    /// what is already reserved. Never negative.
    pub fn left(&self, stream: Stream, now: NaiveDateTime) -> u64 {
        budget::compute_budget(&BudgetInput {
            ledger: &self.ledger,
            plan: self.plan,
            now,
            calls_this_run: self.calls_this_run,
            stream,
            budget_run: self.plan.run,
            no_pace: false,
            rate_limited: self.rate_limited,
            net_down: self.net_down,
            fwd_need: Some(self.fwd_need),
            enrich_used_today: Some(self.enrich_used_today),
            operator_cap: self.operator_cap,
        })
    }

    /// Credits still unspent in the current reservation for `today`.
    pub fn reserved(&self, today: NaiveDate) -> u32 {
        let r = self.reservation.lock().unwrap_or_else(|p| p.into_inner());
        if r.day() == today { r.remaining() } else { 0 }
    }

    /// Make sure at least `min` credits are available for `stream` (up to a
    /// block of [`RESERVE_BLOCK`]), charging the ledger for the new block.
    /// Returns the credits now available; `0` means the policy allows
    /// nothing more right now.
    pub async fn ensure_credits(
        &mut self,
        stream: Stream,
        min: u32,
        now: NaiveDateTime,
    ) -> Result<u32> {
        let today = now.date();
        let have = self.reserved(today);
        if have >= min {
            return Ok(have);
        }
        let want = u64::from(RESERVE_BLOCK.max(min) - have);
        let allowed = self.left(stream, now).min(want);
        let Ok(allowed) = u32::try_from(allowed) else {
            return Ok(have);
        };
        if allowed == 0 {
            return Ok(have);
        }
        self.store.add_requests(today, allowed).await?;
        *self.ledger.entry(today).or_insert(0) += u64::from(allowed);
        self.calls_this_run += u64::from(allowed);
        if stream == Stream::Enrich {
            self.enrich_used_today += u64::from(allowed);
        }
        let mut r = self.reservation.lock().unwrap_or_else(|p| p.into_inner());
        *r = Reservation::new(today, have + allowed);
        Ok(have + allowed)
    }

    /// Snapshot for the cycle log.
    pub fn report(&self, now: NaiveDateTime) -> budget::Report {
        budget::report(
            &self.ledger,
            self.plan,
            now,
            Some(self.fwd_need),
            Some(self.enrich_used_today),
            self.operator_cap,
        )
    }
}

#[cfg(test)]
mod tests;
