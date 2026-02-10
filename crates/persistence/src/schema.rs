// @generated automatically by Diesel CLI.

diesel::table! {
    config_store (instance_id, key) {
        instance_id -> Varchar,
        key -> Varchar,
        value -> Text,
        description -> Nullable<Text>,
        updated_at -> Timestamp,
        created_at -> Timestamp,
    }
}

diesel::table! {
    config_store_history (id) {
        id -> Int4,
        instance_id -> Varchar,
        key -> Varchar,
        old_value -> Nullable<Text>,
        new_value -> Text,
        changed_at -> Timestamp,
    }
}

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
    prediction_records (id) {
        id -> Int4,
        evaluation_period_id -> Varchar,
        token -> Varchar,
        quote_token -> Varchar,
        predicted_price -> Numeric,
        prediction_time -> Timestamp,
        target_time -> Timestamp,
        actual_price -> Nullable<Numeric>,
        mape -> Nullable<Float8>,
        absolute_error -> Nullable<Numeric>,
        evaluated_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    token_rates (id) {
        id -> Int4,
        base_token -> Varchar,
        quote_token -> Varchar,
        rate -> Numeric,
        timestamp -> Timestamp,
        decimals -> Nullable<Int2>,
        rate_calc_near -> Int8,
        swap_path -> Nullable<Jsonb>,
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
        timestamp -> Timestamp,
        evaluation_period_id -> Nullable<Varchar>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    config_store,
    config_store_history,
    evaluation_periods,
    pool_info,
    prediction_records,
    token_rates,
    trade_transactions,
);
