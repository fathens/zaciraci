use super::*;

crate::batch::enforce_cols_matches_fields!(NewDbPoolInfo {
    pool_id,
    pool_kind,
    token_account_ids,
    amounts,
    total_fee,
    shares_total_supply,
    amp,
    timestamp,
});
