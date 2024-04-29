// @generated automatically by Diesel CLI.

diesel::table! {
    counter (id) {
        id -> Int4,
        value -> Int4,
    }
}

diesel::table! {
    pool_info (id) {
        id -> Int4,
        pool_kind -> Varchar,
        token_account_ids -> Array<Text>,
        amounts -> Array<Numeric>,
        total_fee -> Int8,
        shares_total_supply -> Numeric,
        amp -> Numeric,
        updated_at -> Timestamp,
    }
}

diesel::allow_tables_to_appear_in_same_query!(counter, pool_info,);
