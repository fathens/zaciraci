use super::*;

/// Adding or removing an `Insertable` field on `NewPredictionRecord` without
/// updating `COLS` triggers a compile error here: the destructuring pattern
/// is exhaustive, and the array literal's length is type-checked against
/// `NewPredictionRecord::COLS`. Together they refuse to compile until the
/// bind-parameter count and the field list are realigned.
#[test]
fn cols_matches_struct_fields() {
    fn _enforce(v: NewPredictionRecord) {
        let NewPredictionRecord {
            token,
            quote_token,
            predicted_price,
            data_cutoff_time,
            target_time,
            created_at,
        } = v;
        let _: [(); NewPredictionRecord::COLS.get()] = [
            {
                let _ = token;
            },
            {
                let _ = quote_token;
            },
            {
                let _ = predicted_price;
            },
            {
                let _ = data_cutoff_time;
            },
            {
                let _ = target_time;
            },
            {
                let _ = created_at;
            },
        ];
    }
    let _: fn(NewPredictionRecord) = _enforce;
}
