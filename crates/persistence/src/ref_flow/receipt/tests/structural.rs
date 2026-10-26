use super::*;

crate::batch::enforce_cols_matches_fields!(NewDbRefReceipt {
    receipt_id,
    tx_hash,
    block_height,
    block_time,
    predecessor,
    methods,
    ok,
});

mod tx_row {
    use super::*;

    crate::batch::enforce_cols_matches_fields!(NewDbRefTx {
        tx_hash,
        block_time
    });
}
