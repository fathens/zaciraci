use super::*;

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
