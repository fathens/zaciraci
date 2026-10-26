use super::*;

/// Deterministic 32-byte hash encodings for test ids: `hash(n)` is the
/// base58 form of `[n; 32]`-like bytes, unique per `n`.
pub fn hash(n: u32) -> String {
    let mut bytes = [0u8; 32];
    bytes[..4].copy_from_slice(&n.to_be_bytes());
    bytes[31] = 1; // avoid the all-zero / leading-'1' degenerate case
    bs58::encode(bytes).into_string()
}

pub fn receipt_id(n: u32) -> ReceiptId {
    ReceiptId::new(&hash(1_000_000 + n)).unwrap()
}

pub fn tx_hash(n: u32) -> TxHash {
    TxHash::new(&hash(n)).unwrap()
}

pub fn ts(secs: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_792_800_000 + secs, 0)
        .unwrap()
        .naive_utc()
}

/// A receipt `n` for transaction `tx` at `ts(secs)`.
pub fn make_receipt(n: u32, tx: u32, secs: i64, ok: bool) -> RefReceipt {
    RefReceipt::new(RefReceiptParts {
        receipt_id: receipt_id(n),
        tx_hash: tx_hash(tx),
        block_height: 221_000_000 + u64::from(n),
        block_time: ts(secs),
        predecessor: format!("sender{n}.near").parse().unwrap(),
        methods: vec!["swap".to_owned()],
        ok,
    })
    .unwrap()
}
