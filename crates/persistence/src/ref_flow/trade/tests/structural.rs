use super::*;

crate::batch::enforce_cols_matches_fields!(NewDbRefTrade {
    receipt_id,
    tx_hash,
    block_height,
    block_time,
    sender,
    route,
    pool_ids,
    hop_tokens_in,
    hop_tokens_out,
    fee_bps,
    token_in,
    amount_in,
    token_out,
    amount_out,
});
