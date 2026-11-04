use super::*;
use crate::budget::STANDARD;
use crate::credit::CreditSource;

/// In-memory ledger that records every charge.
#[derive(Debug, Default, Clone)]
struct MemLedger {
    charges: Arc<Mutex<Vec<(NaiveDate, u32)>>>,
}

impl LedgerStore for MemLedger {
    async fn add_requests(&self, day: NaiveDate, requests: u32) -> Result<u64> {
        let mut c = self.charges.lock().unwrap();
        c.push((day, requests));
        Ok(c.iter()
            .filter(|(d, _)| *d == day)
            .map(|(_, n)| u64::from(*n))
            .sum())
    }
}

/// Ledger whose charge call draws `draws` credits from the tracker's own
/// reservation while the charge is in flight, like concurrent enrich
/// workers do.
#[derive(Debug, Default, Clone)]
struct DrawingLedger {
    inner: MemLedger,
    reservation: Arc<Mutex<Option<Arc<Mutex<Reservation>>>>>,
    draws: u32,
}

impl LedgerStore for DrawingLedger {
    async fn add_requests(&self, day: NaiveDate, requests: u32) -> Result<u64> {
        if let Some(r) = self.reservation.lock().unwrap().as_ref() {
            let source: &dyn CreditSource = &**r;
            for _ in 0..self.draws {
                drop(
                    source
                        .take(day)
                        .expect("credit available during the charge"),
                );
            }
        }
        self.inner.add_requests(day, requests).await
    }
}

fn at(h: u32, m: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 26)
        .unwrap()
        .and_hms_opt(h, m, 0)
        .unwrap()
}

fn tracker(store: MemLedger, ledger: Ledger, cap: Option<u64>) -> BudgetTracker<MemLedger> {
    BudgetTracker::new(
        store,
        &STANDARD,
        ledger,
        StreamStats {
            fwd_7d_mean: 1_000,
            fwd_last_day: 900,
            enrich_used_today: 0,
        },
        cap,
        at(12, 0).date(),
    )
}

#[tokio::test]
async fn reserves_a_block_charges_the_ledger_and_feeds_the_client() {
    let store = MemLedger::default();
    let mut t = tracker(store.clone(), Ledger::new(), None);
    let now = at(12, 0);
    assert_eq!(t.reserved(now.date()), 0);

    let got = t.ensure_credits(Stream::Forward, 1, now).await.unwrap();
    assert_eq!(got, RESERVE_BLOCK);
    assert_eq!(
        store.charges.lock().unwrap().as_slice(),
        &[(now.date(), RESERVE_BLOCK)]
    );
    assert_eq!(t.calls_this_run(), u64::from(RESERVE_BLOCK));

    // The client draws from the same reservation.
    let credits = t.credits();
    let source: &dyn CreditSource = &*credits;
    assert!(source.take(now.date()).is_some());
    assert_eq!(t.reserved(now.date()), RESERVE_BLOCK - 1);

    // Enough left: no new charge.
    assert_eq!(
        t.ensure_credits(Stream::Forward, 1, now).await.unwrap(),
        RESERVE_BLOCK - 1
    );
    assert_eq!(store.charges.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn tops_up_only_what_the_policy_allows() {
    let store = MemLedger::default();
    let now = at(12, 0);
    // Forward has 7 requests of daily allowance left.
    let ledger = Ledger::from([(now.date(), STANDARD.day_max - 7)]);
    let mut t = tracker(store.clone(), ledger, None);
    assert_eq!(t.ensure_credits(Stream::Forward, 1, now).await.unwrap(), 7);
    assert_eq!(store.charges.lock().unwrap().as_slice(), &[(now.date(), 7)]);
    // Spend them all, then nothing more is granted.
    for _ in 0..7 {
        drop(t.credits().take(now.date()).unwrap());
    }
    assert_eq!(t.ensure_credits(Stream::Forward, 1, now).await.unwrap(), 0);
    assert_eq!(
        store.charges.lock().unwrap().len(),
        1,
        "no zero-sized charge"
    );
}

#[tokio::test]
async fn a_block_from_yesterday_is_forfeited_at_midnight() {
    let store = MemLedger::default();
    let mut t = tracker(store.clone(), Ledger::new(), None);
    let yesterday = at(23, 59) - chrono::TimeDelta::days(1);
    t.ensure_credits(Stream::Forward, 1, yesterday)
        .await
        .unwrap();
    assert_eq!(t.reserved(yesterday.date()), RESERVE_BLOCK);

    let today = at(0, 0);
    assert_eq!(t.reserved(today.date()), 0, "yesterday's block is unusable");
    assert!(t.credits().take(today.date()).is_none());
    t.ensure_credits(Stream::Forward, 1, today).await.unwrap();
    let charges = store.charges.lock().unwrap();
    assert_eq!(charges.len(), 2);
    assert_eq!(charges[1].0, today.date());
}

#[tokio::test]
async fn rate_limit_and_network_loss_stop_all_reservations() {
    let store = MemLedger::default();
    let mut t = tracker(store.clone(), Ledger::new(), None);
    let now = at(12, 0);
    t.set_rate_limited();
    assert!(t.is_rate_limited());
    assert_eq!(t.left(Stream::Forward, now), 0);
    assert_eq!(t.ensure_credits(Stream::Forward, 1, now).await.unwrap(), 0);
    assert!(store.charges.lock().unwrap().is_empty());

    let mut t = tracker(store.clone(), Ledger::new(), None);
    t.set_net_down();
    assert_eq!(t.ensure_credits(Stream::Enrich, 1, now).await.unwrap(), 0);
}

#[tokio::test]
async fn enrich_reservations_count_against_the_enrich_allotment() {
    let store = MemLedger::default();
    let now = at(12, 0);
    let mut t = tracker(store.clone(), Ledger::new(), None);
    let before = t.left(Stream::Enrich, now);
    let got = t.ensure_credits(Stream::Enrich, 1, now).await.unwrap();
    assert_eq!(got, RESERVE_BLOCK);
    // Both the day ledger and the enrich allotment moved by the block.
    assert_eq!(
        t.left(Stream::Enrich, now),
        before - u64::from(RESERVE_BLOCK)
    );
    assert_eq!(t.report(now).enrich_used, Some(u64::from(RESERVE_BLOCK)));
}

#[tokio::test]
async fn operator_cap_bounds_the_day() {
    let store = MemLedger::default();
    let now = at(12, 0);
    let ledger = Ledger::from([(now.date(), 45)]);
    let mut t = tracker(store, ledger, Some(50));
    assert_eq!(t.report(now).day_allow, 50);
    assert_eq!(t.ensure_credits(Stream::Forward, 1, now).await.unwrap(), 5);
}

#[tokio::test]
async fn minimum_larger_than_a_block_is_honoured() {
    let store = MemLedger::default();
    let mut t = tracker(store, Ledger::new(), None);
    let got = t
        .ensure_credits(Stream::Forward, RESERVE_BLOCK * 3, at(12, 0))
        .await
        .unwrap();
    assert_eq!(got, RESERVE_BLOCK * 3);
}

#[tokio::test]
async fn credits_drawn_during_the_charge_are_not_handed_out_again() {
    let store = DrawingLedger {
        draws: 3,
        ..Default::default()
    };
    let now = at(12, 0);
    let mut t = BudgetTracker::new(
        store.clone(),
        &STANDARD,
        Ledger::new(),
        StreamStats {
            fwd_7d_mean: 1_000,
            fwd_last_day: 900,
            enrich_used_today: 0,
        },
        None,
        now.date(),
    );
    // First block: nothing to draw from yet.
    *store.reservation.lock().unwrap() = None;
    assert_eq!(
        t.ensure_credits(Stream::Enrich, 1, now).await.unwrap(),
        RESERVE_BLOCK
    );
    // Spend down to 5, then ask for 16: the block is topped up to
    // RESERVE_BLOCK (a charge of 15) while 3 more are drawn in flight.
    let credits = t.credits();
    let source: &dyn CreditSource = &*credits;
    for _ in 0..(RESERVE_BLOCK - 5) {
        drop(source.take(now.date()).unwrap());
    }
    *store.reservation.lock().unwrap() = Some(Arc::clone(&credits));
    let got = t.ensure_credits(Stream::Enrich, 16, now).await.unwrap();
    assert_eq!(got, 5 - 3 + 15, "5 left, 3 drawn meanwhile, 15 added");
    assert_eq!(t.reserved(now.date()), got);
    let charged: u64 = store
        .inner
        .charges
        .lock()
        .unwrap()
        .iter()
        .map(|(_, n)| u64::from(*n))
        .sum();
    assert_eq!(
        charged,
        u64::from(RESERVE_BLOCK + 15),
        "every credit handed out was charged exactly once"
    );
}
