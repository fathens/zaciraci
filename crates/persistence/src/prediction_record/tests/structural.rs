use super::*;

crate::batch::enforce_cols_matches_fields!(NewPredictionRecord {
    token,
    quote_token,
    predicted_price,
    data_cutoff_time,
    target_time,
    created_at,
});
