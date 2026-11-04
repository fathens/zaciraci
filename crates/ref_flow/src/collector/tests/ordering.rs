use super::*;

fn pending(n: u32, secs: i64) -> PendingTx {
    PendingTx {
        tx_hash: TxHash::new(&hash(n)).unwrap(),
        block_time: chrono::DateTime::from_timestamp(1_792_800_000 + secs, 0)
            .unwrap()
            .naive_utc(),
    }
}

#[test]
fn round_robin_interleaves_buckets_newest_first() {
    // Bucket A (older): 3 txs; bucket B (newer, +30 min): 2 txs; bucket C
    // (newest, +60 min): 1 tx.
    let candidates = vec![
        pending(1, 0),
        pending(2, 60),
        pending(3, 120),
        pending(4, 1800),
        pending(5, 1860),
        pending(6, 3600),
    ];
    let ordered = round_robin_by_bucket(candidates);
    let secs: Vec<i64> = ordered
        .iter()
        .map(|p| p.block_time.and_utc().timestamp() - 1_792_800_000)
        .collect();
    // Round 1: one from C, one from B, one from A; round 2: B, A; round 3: A.
    assert_eq!(secs.len(), 6);
    assert!(secs[0] >= 3600, "newest bucket first: {secs:?}");
    assert!((1800..3600).contains(&secs[1]), "{secs:?}");
    assert!(secs[2] < 1800, "{secs:?}");
    assert!((1800..3600).contains(&secs[3]), "{secs:?}");
    assert!(secs[4] < 1800 && secs[5] < 1800, "{secs:?}");
}

#[test]
fn round_robin_is_deterministic_within_a_bucket() {
    let a = round_robin_by_bucket(vec![pending(3, 0), pending(1, 10), pending(2, 20)]);
    let b = round_robin_by_bucket(vec![pending(2, 20), pending(3, 0), pending(1, 10)]);
    let ids = |v: &[PendingTx]| {
        v.iter()
            .map(|p| p.tx_hash.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&a), ids(&b));
    let mut sorted = ids(&a);
    sorted.sort();
    assert_eq!(ids(&a), sorted, "ordered by hash within the bucket");
    assert!(round_robin_by_bucket(vec![]).is_empty());
}

#[test]
fn effective_retention_never_drops_below_the_persistence_minimum() {
    let mut cfg = config();
    cfg.retention_days = 0;
    assert_eq!(
        cfg.effective_retention_days(),
        persistence::ref_flow::MIN_RETENTION_DAYS
    );
    cfg.retention_days = 10;
    assert_eq!(
        cfg.effective_retention_days(),
        persistence::ref_flow::MIN_RETENTION_DAYS
    );
    cfg.retention_days = 90;
    assert_eq!(cfg.effective_retention_days(), 90);
}
