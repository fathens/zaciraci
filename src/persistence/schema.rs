// @generated automatically by Diesel CLI.

diesel::table! {
    evaluation_periods (id) {
        id -> Int4,
        period_id -> Varchar,
        start_time -> Timestamp,
        initial_value -> Numeric,
        selected_tokens -> Nullable<Array<Nullable<Text>>>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    pool_info (id) {
        id -> Int4,
        pool_id -> Int4,
        pool_kind -> Varchar,
        token_account_ids -> Jsonb,
        amounts -> Jsonb,
        total_fee -> Int4,
        shares_total_supply -> Jsonb,
        amp -> Int8,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    token_rates (id) {
        id -> Int4,
        base_token -> Varchar,
        quote_token -> Varchar,
        rate -> Numeric,
        timestamp -> Timestamp,
    }
}

diesel::table! {
    trade_transactions (tx_id) {
        tx_id -> Varchar,
        trade_batch_id -> Varchar,
        from_token -> Varchar,
        from_amount -> Numeric,
        to_token -> Varchar,
        to_amount -> Numeric,
        price_yocto_near -> Numeric,
        timestamp -> Timestamp,
        evaluation_period_id -> Nullable<Varchar>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    evaluation_periods,
    pool_info,
    token_rates,
    trade_transactions,
);
