//! End-to-end cycles against the test database with a fake API.

use super::*;
use crate::client::ListPage;
use persistence::ref_flow::test_support::clean_tables;
use serial_test::serial;

fn no_save() -> impl FnMut(&CollectorState) -> futures::future::BoxFuture<'static, Result<()>> {
    |_| Box::pin(async { Ok(()) })
}

/// Forward cursor one hour in the past. The cycle classifies gaps by
/// their distance from the wall clock (recent = newer than 72 h) and the
/// cleanup pass deletes rows older than the retention, so a fixed epoch
/// would make these tests flip as the calendar moves on.
fn recent_since() -> i64 {
    Utc::now().timestamp() - 3600
}

/// Two forward pages: the newest page has receipts above `since`, the
/// second page reaches a receipt at `since` (complete).
fn two_pages(since_secs: i64) -> Vec<ListPage> {
    vec![
        ListPage {
            items: vec![
                list_item(1, 1, ns(since_secs + 1800), true),
                list_item(2, 2, ns(since_secs + 1200), true),
                list_item(3, 3, ns(since_secs + 600), false),
            ],
            next_cursor: Some("p2".to_owned()),
        },
        ListPage {
            items: vec![
                list_item(4, 4, ns(since_secs + 300), true),
                list_item(5, 5, ns(since_secs), true), // at since: stop
                list_item(6, 6, ns(since_secs - 300), true),
            ],
            next_cursor: Some("p3".to_owned()),
        },
    ]
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn fresh_cycle_lists_ingests_and_enriches() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let api = Arc::new(FakeApi::with_pages(two_pages(since)));
    // Enrich answers for the successful txs 1, 2, 4 (5 and 6 are at/below
    // since and never listed).
    for (tx, n, secs) in [
        (1, 1, since + 1800),
        (2, 2, since + 1200),
        (4, 4, since + 300),
    ] {
        api.answer(&hash(tx), TxAnswer::Receipts(swap_tree(tx, n, ns(secs))));
    }
    let ledger = MemLedger::default();
    let mut cycle = Cycle::new(Arc::clone(&api), tracker(ledger.clone(), None), config());
    let mut state = CollectorState::new(ns(since));
    let mut saves = 0;
    let report = cycle
        .run(&mut state, &mut |_| {
            saves += 1;
            Box::pin(async { Ok(()) })
        })
        .await?;

    // Forward: two pages, 4 receipts kept (1,2,3 ok/failed + 4), complete.
    assert_eq!(report.forward.pages, 2);
    assert_eq!(report.forward.kept, 4);
    assert!(report.forward.complete);
    assert_eq!(state.since_ns, ns(since + 1800));
    assert!(state.gaps.is_empty());
    assert!(saves >= 2, "state saved after forward and gaps");
    assert_eq!(
        api.list_calls.lock().unwrap().as_slice(),
        &[ListFrom::Newest, ListFrom::Cursor("p2".to_owned())]
    );

    // Enrich: frame rows for the 3 ok txs, all fetched and recorded.
    assert_eq!(report.enrich.candidates, 3);
    assert_eq!(report.enrich.fetched, 3);
    assert_eq!(report.enrich.recorded, 3);
    assert_eq!(report.enrich.trades, 3);
    assert_eq!(report.enrich.failed, 0);
    assert_eq!(report.enrich.stats.accepted, 3);
    let mut fetched = api.txn_calls.lock().unwrap().clone();
    fetched.sort();
    let mut expected = vec![hash(1), hash(2), hash(4)];
    expected.sort();
    assert_eq!(fetched, expected);

    // Persisted: txs enriched, trades present, runs rows, ledger charged.
    for tx in [1, 2, 4] {
        let rec = tx_store::get(&TxHash::new(&hash(tx)).unwrap())
            .await?
            .unwrap();
        assert!(rec.is_enriched(), "tx {tx}");
        assert_eq!(rec.trade_count, Some(1));
    }
    assert!(
        tx_store::get(&TxHash::new(&hash(3)).unwrap())
            .await?
            .is_none(),
        "failed receipt: no frame row"
    );
    let trades = trade_store::get_by_tx_hash(&TxHash::new(&hash(2)).unwrap()).await?;
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].hops()[0].pool_id, 6065);
    let latest = persistence::ref_flow::runs::latest(None).await?.unwrap();
    assert_eq!(latest.stream, RunStream::Enrich);
    assert_eq!(latest.trades, 3);
    let fwd = persistence::ref_flow::runs::latest(Some(RunStream::Fwd))
        .await?
        .unwrap();
    assert_eq!(fwd.pages, 2);
    assert_eq!(fwd.receipts, 4);
    // 2 list + 3 enrich requests were sent; the ledger holds at least that
    // (blocks are pre-charged).
    assert!(ledger.total() >= 5);
    assert!(!report.rate_limited && !report.net_down);

    // Second cycle: nothing new (the fake has no more pages), nothing pending.
    let api2 = Arc::new(FakeApi::with_pages(vec![ListPage {
        items: vec![list_item(1, 1, ns(since + 1800), true)],
        next_cursor: None,
    }]));
    let mut cycle = Cycle::new(api2, tracker(ledger.clone(), None), config());
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert_eq!(report.forward.kept, 0, "item at since is not re-ingested");
    assert!(report.forward.complete);
    assert_eq!(report.enrich.candidates, 0);
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn page_cap_registers_a_gap_that_the_next_cycle_closes() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let api = Arc::new(FakeApi::with_pages(two_pages(since)));
    let mut cfg = config();
    cfg.fwd_max_pages = 1;
    let mut cycle = Cycle::new(
        Arc::clone(&api),
        tracker(MemLedger::default(), None),
        cfg.clone(),
    );
    let mut state = CollectorState::new(ns(since));
    // Capture every saved snapshot: the first one is taken right after
    // the forward pass, before the gap is closed.
    let snapshots: Arc<std::sync::Mutex<Vec<CollectorState>>> = Arc::default();
    let sink = Arc::clone(&snapshots);
    let report = cycle
        .run(&mut state, &mut move |s: &CollectorState| {
            sink.lock().unwrap().push(s.clone());
            Box::pin(async { Ok(()) })
        })
        .await?;
    assert_eq!(report.forward.pages, 1);
    assert_eq!(report.forward.stop, Some(Stop::PageCap));
    assert!(!report.forward.complete);
    assert_eq!(state.since_ns, ns(since + 1800));
    let after_forward = snapshots.lock().unwrap()[0].clone();
    assert_eq!(after_forward.gaps.len(), 1);
    assert_eq!(after_forward.gaps[0].cursor.as_deref(), Some("p2"));
    assert_eq!(after_forward.gaps[0].until_ns, ns(since));
    assert_eq!(after_forward.gaps[0].oldest_ns, Some(ns(since + 600)));
    // The gap is recent, so the same cycle already closed it using the
    // cursor: page 2 reaches `since`.
    assert_eq!(report.recent_gaps, 1);
    assert!(state.gaps.is_empty());
    let calls = api.list_calls.lock().unwrap().clone();
    assert_eq!(calls[1], ListFrom::Cursor("p2".to_owned()));
    // Receipt 4 (from the gap page) was ingested.
    assert!(
        tx_store::get(&TxHash::new(&hash(4)).unwrap())
            .await?
            .is_some()
    );
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn unusable_answers_count_attempts_and_isolate_poison_transactions() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let api = Arc::new(FakeApi::with_pages(vec![ListPage {
        items: vec![
            list_item(1, 1, ns(since + 60), true),
            list_item(2, 2, ns(since + 30), true),
        ],
        next_cursor: None,
    }]));
    api.answer(&hash(1), TxAnswer::Status(500));
    api.answer(
        &hash(2),
        TxAnswer::Receipts(swap_tree(2, 2, ns(since + 30))),
    );
    let mut state = CollectorState::new(ns(since));
    let cfg = config(); // max_enrich_attempts = 2

    let mut cycle = Cycle::new(
        Arc::clone(&api),
        tracker(MemLedger::default(), None),
        cfg.clone(),
    );
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert_eq!(report.enrich.failed, 1);
    assert_eq!(report.enrich.isolated, 0);
    assert_eq!(report.enrich.recorded, 1);
    let rec = tx_store::get(&TxHash::new(&hash(1)).unwrap())
        .await?
        .unwrap();
    assert_eq!(rec.attempts, 1);
    assert!(!rec.is_enriched());

    // Next cycle (no new receipts): tx 1 is retried and isolated.
    let api2 = Arc::new(FakeApi::default());
    api2.answer(&hash(1), TxAnswer::Status(500));
    let mut cycle = Cycle::new(api2, tracker(MemLedger::default(), None), cfg);
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert_eq!(report.enrich.candidates, 1);
    assert_eq!(report.enrich.isolated, 1);
    let rec = tx_store::get(&TxHash::new(&hash(1)).unwrap())
        .await?
        .unwrap();
    assert_eq!(rec.attempts, 2);
    assert!(rec.is_enriched());
    assert_eq!(rec.trade_count, Some(0));
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn rate_limit_aborts_and_sets_cooldown() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let api = Arc::new(FakeApi::with_pages(vec![ListPage {
        items: vec![list_item(1, 1, ns(since + 60), true)],
        next_cursor: None,
    }]));
    api.answer(&hash(1), TxAnswer::RateLimited);
    let mut state = CollectorState::new(ns(since));
    let mut cycle = Cycle::new(api, tracker(MemLedger::default(), None), config());
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert!(report.rate_limited);
    assert_eq!(report.enrich.stop, Some(Stop::RateLimited));
    let until = state.cooldown_until.unwrap();
    assert!(until > Utc::now().timestamp() + 50 * 60);
    // The transaction stays pending (no attempt counted against it).
    let rec = tx_store::get(&TxHash::new(&hash(1)).unwrap())
        .await?
        .unwrap();
    assert_eq!(rec.attempts, 0);
    assert!(!rec.is_enriched());
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn exhausted_budget_stops_before_any_request() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let api = Arc::new(FakeApi::with_pages(two_pages(since)));
    let mut state = CollectorState::new(ns(since));
    // Operator cap 0: nothing may be spent.
    let mut cycle = Cycle::new(
        Arc::clone(&api),
        tracker(MemLedger::default(), Some(0)),
        config(),
    );
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert_eq!(report.forward.stop, Some(Stop::Budget));
    assert_eq!(report.forward.pages, 0);
    assert!(api.list_calls.lock().unwrap().is_empty());
    assert_eq!(state.since_ns, ns(since), "cursor untouched");
    Ok(())
}

#[tokio::test]
#[serial(ref_flow_db)]
async fn network_failures_leave_transactions_pending() -> Result<()> {
    clean_tables().await?;
    let since = recent_since();
    let items: Vec<_> = (1..=4)
        .map(|n| list_item(n, n, ns(since + i64::from(n)), true))
        .collect();
    let api = Arc::new(FakeApi::with_pages(vec![ListPage {
        items,
        next_cursor: None,
    }]));
    for n in 1..=4 {
        api.answer(&hash(n), TxAnswer::Network);
    }
    let mut state = CollectorState::new(ns(since));
    let mut cycle = Cycle::new(api, tracker(MemLedger::default(), None), config());
    let report = cycle.run(&mut state, &mut no_save()).await?;
    assert_eq!(report.enrich.stop, Some(Stop::NetworkDown));
    assert!(report.net_down);
    assert_eq!(
        report.enrich.failed, 0,
        "network errors are not the transaction's fault"
    );
    for n in 1..=4 {
        let rec = tx_store::get(&TxHash::new(&hash(n)).unwrap())
            .await?
            .unwrap();
        assert_eq!(rec.attempts, 0);
    }
    Ok(())
}
