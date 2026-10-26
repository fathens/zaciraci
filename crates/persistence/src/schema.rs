// @generated automatically by Diesel CLI.

diesel::table! {
    authorized_users (id) {
        id -> Int4,
        email -> Varchar,
        role -> Varchar,
        created_at -> Timestamp,
        updated_at -> Timestamp,
    }
}

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
    portfolio_holdings (id) {
        id -> Int4,
        evaluation_period_id -> Varchar,
        timestamp -> Timestamp,
        token_holdings -> Jsonb,
        created_at -> Timestamp,
    }
}

diesel::table! {
    prediction_records (id) {
        id -> Int4,
        token -> Varchar,
        quote_token -> Varchar,
        predicted_price -> Numeric,
        data_cutoff_time -> Timestamp,
        target_time -> Timestamp,
        actual_price -> Nullable<Numeric>,
        mape -> Nullable<Float8>,
        absolute_error -> Nullable<Numeric>,
        evaluated_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
    }
}

diesel::table! {
    ref_flow_receipts (receipt_id) {
        receipt_id -> Varchar,
        tx_hash -> Varchar,
        block_height -> Int8,
        block_time -> Timestamp,
        predecessor -> Varchar,
        methods -> Array<Nullable<Text>>,
        ok -> Bool,
    }
}

diesel::table! {
    ref_flow_trades (receipt_id) {
        receipt_id -> Varchar,
        tx_hash -> Varchar,
        block_height -> Int8,
        block_time -> Timestamp,
        sender -> Varchar,
        route -> Varchar,
        pool_ids -> Array<Nullable<Int4>>,
        hop_tokens_in -> Array<Nullable<Text>>,
        hop_tokens_out -> Array<Nullable<Text>>,
        fee_bps -> Nullable<Array<Nullable<Int4>>>,
        token_in -> Varchar,
        amount_in -> Numeric,
        token_out -> Varchar,
        amount_out -> Numeric,
    }
}

diesel::table! {
    ref_flow_txs (tx_hash) {
        tx_hash -> Varchar,
        block_time -> Timestamp,
        enriched_at -> Nullable<Timestamp>,
        attempts -> Int2,
        trade_count -> Nullable<Int2>,
    }
}

diesel::table! {
    token_rates (id) {
        id -> Int4,
        base_token -> Varchar,
        quote_token -> Varchar,
        rate -> Numeric,
        timestamp -> Timestamp,
        decimals -> Int2,
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
        evaluation_period_id -> Varchar,
        actual_to_amount -> Nullable<Numeric>,
    }
}

diesel::joinable!(ref_flow_trades -> ref_flow_txs (tx_hash));

diesel::allow_tables_to_appear_in_same_query!(
    authorized_users,
    config_store,
    config_store_history,
    evaluation_periods,
    pool_info,
    portfolio_holdings,
    prediction_records,
    ref_flow_receipts,
    ref_flow_trades,
    ref_flow_txs,
    token_rates,
    trade_transactions,
);
