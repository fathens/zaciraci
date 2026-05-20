use super::*;

crate::batch::enforce_cols_matches_fields!(TradeTransaction {
    tx_id,
    trade_batch_id,
    from_token,
    from_amount,
    to_token,
    to_amount,
    timestamp,
    evaluation_period_id,
    actual_to_amount,
});
