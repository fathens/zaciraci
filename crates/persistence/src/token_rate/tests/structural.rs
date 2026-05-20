use super::*;

crate::batch::enforce_cols_matches_fields!(NewDbTokenRate {
    base_token,
    quote_token,
    rate,
    timestamp,
    decimals,
    rate_calc_near,
    swap_path,
});
