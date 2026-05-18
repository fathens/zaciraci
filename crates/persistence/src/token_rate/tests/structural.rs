use super::*;

/// Adding or removing an `Insertable` field on `NewDbTokenRate` without
/// updating `COLS` triggers a compile error here: the destructuring pattern
/// is exhaustive, and the array literal's length is type-checked against
/// `NewDbTokenRate::COLS`. Together they refuse to compile until the
/// bind-parameter count and the field list are realigned.
#[test]
fn cols_matches_struct_fields() {
    fn _enforce(v: NewDbTokenRate) {
        let NewDbTokenRate {
            base_token,
            quote_token,
            rate,
            timestamp,
            decimals,
            rate_calc_near,
            swap_path,
        } = v;
        let _: [(); NewDbTokenRate::COLS.get()] = [
            {
                let _ = base_token;
            },
            {
                let _ = quote_token;
            },
            {
                let _ = rate;
            },
            {
                let _ = timestamp;
            },
            {
                let _ = decimals;
            },
            {
                let _ = rate_calc_near;
            },
            {
                let _ = swap_path;
            },
        ];
    }
    let _: fn(NewDbTokenRate) = _enforce;
}
