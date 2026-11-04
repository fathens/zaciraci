//! Deterministic ids and times for `ref_flow` unit tests.

use chrono::NaiveDateTime;
use common::types::{ReceiptId, TxHash};

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
