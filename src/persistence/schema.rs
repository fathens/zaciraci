// @generated automatically by Diesel CLI.

diesel::table! {
    token_rates (id) {
        id -> Int4,
        base_token -> Varchar,
        quote_token -> Varchar,
        rate -> Numeric,
        timestamp -> Timestamp,
    }
}
