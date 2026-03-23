use super::*;
use bigdecimal::BigDecimal;
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, ViewContract};
use chrono::{TimeZone, Utc};
use common::types::{TokenAmount, YoctoValue};
use near_crypto::InMemorySigner;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};

fn default_sim_day() -> Arc<Mutex<DateTime<Utc>>> {
    Arc::new(Mutex::new(
        Utc.with_ymd_and_hms(2025, 6, 15, 0, 0, 0).unwrap(),
    ))
}

fn make_client_with_holdings(cash: u128, holdings: Vec<(&str, u128, u8)>) -> SimulationClient {
    let mut state = PortfolioState::new(YoctoValue::from_yocto(BigDecimal::from(cash)));
    for (token, amount, decimals) in holdings {
        let token_account: TokenAccount = token.parse().unwrap();
        state.holdings.insert(
            token_account,
            TokenAmount::from_smallest_units(BigDecimal::from(amount), decimals),
        );
    }
    let portfolio = Arc::new(Mutex::new(state));
    SimulationClient::new(portfolio, cash, default_sim_day())
}

fn make_client(cash: u128) -> SimulationClient {
    make_client_with_holdings(cash, vec![])
}

fn make_client_with_portfolio(portfolio: Arc<Mutex<PortfolioState>>) -> SimulationClient {
    SimulationClient::new(portfolio, 0, default_sim_day())
}

fn test_signer() -> InMemorySigner {
    let account_id: AccountId = "sim.near".parse().unwrap();
    let signer =
        near_crypto::InMemorySigner::from_seed(account_id, near_crypto::KeyType::ED25519, "test");
    match signer {
        near_crypto::Signer::InMemory(s) => s,
        _ => panic!("expected InMemorySigner"),
    }
}

fn wnear_str() -> String {
    blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string()
}

#[tokio::test]
async fn view_contract_get_deposits_returns_cash_and_holdings() {
    let cash = 50_000_000_000_000_000_000_000_000u128; // 50 NEAR
    let client = make_client_with_holdings(cash, vec![("usdt.tether-token.near", 1_000_000, 6)]);

    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();
    let result = client
        .view_contract(&receiver, "get_deposits", &serde_json::json!({}))
        .await
        .unwrap();

    let deposits: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&result.result).unwrap();

    // Cash as wrap.near
    let wnear = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    assert_eq!(
        deposits[&wnear],
        serde_json::Value::String(cash.to_string())
    );

    // Token holding
    assert_eq!(
        deposits["usdt.tether-token.near"],
        serde_json::Value::String("1000000".to_string())
    );
}

#[tokio::test]
async fn view_contract_ft_metadata_returns_decimals() {
    let client = make_client(0);

    let receiver: AccountId = "usdt.tether-token.near".parse().unwrap();
    let result = client
        .view_contract(&receiver, "ft_metadata", &serde_json::json!({}))
        .await
        .unwrap();

    let metadata: serde_json::Value = serde_json::from_slice(&result.result).unwrap();
    // decimals defaults to 24 when no decimals are stored (empty portfolio)
    assert_eq!(metadata["decimals"], 24);
    assert_eq!(metadata["spec"], "ft-1.0.0");
}

#[tokio::test]
async fn view_contract_ft_metadata_returns_stored_decimals() {
    let client = make_client_with_holdings(0, vec![("usdt.tether-token.near", 1_000_000, 6)]);

    let receiver: AccountId = "usdt.tether-token.near".parse().unwrap();
    let result = client
        .view_contract(&receiver, "ft_metadata", &serde_json::json!({}))
        .await
        .unwrap();

    let metadata: serde_json::Value = serde_json::from_slice(&result.result).unwrap();
    // decimals should come from holdings (6 for usdt)
    assert_eq!(metadata["decimals"], 6);
}

#[tokio::test]
async fn view_contract_ft_balance_of_returns_large_value() {
    let client = make_client(0);

    let receiver: AccountId = "usdt.tether-token.near".parse().unwrap();
    let result = client
        .view_contract(
            &receiver,
            "ft_balance_of",
            &serde_json::json!({"account_id": "sim.near"}),
        )
        .await
        .unwrap();

    let balance: U128 = serde_json::from_slice(&result.result).unwrap();
    assert_eq!(balance.0, 10u128.pow(30));
}

#[tokio::test]
async fn view_contract_storage_balance_of_returns_some() {
    let client = make_client(0);

    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();
    let result = client
        .view_contract(
            &receiver,
            "storage_balance_of",
            &serde_json::json!({"account_id": "sim.near"}),
        )
        .await
        .unwrap();

    let info: serde_json::Value = serde_json::from_slice(&result.result).unwrap();
    // Should have a total field (non-null response)
    assert!(info.get("total").is_some());
}

#[tokio::test]
async fn view_contract_unknown_method_returns_empty() {
    let client = make_client(0);

    let receiver: AccountId = "some.near".parse().unwrap();
    let result = client
        .view_contract(&receiver, "nonexistent_method", &serde_json::json!({}))
        .await
        .unwrap();

    let balance: U128 = serde_json::from_slice(&result.result).unwrap();
    assert_eq!(balance.0, 0);
}

#[tokio::test]
async fn get_native_amount_returns_initial_capital() {
    let initial = 100_000_000_000_000_000_000_000_000u128; // 100 NEAR
    let client = make_client(initial);

    let account: AccountId = "sim.near".parse().unwrap();
    let amount = client.get_native_amount(&account).await.unwrap();
    assert_eq!(amount.as_yoctonear(), initial);
}

// ---------------------------------------------------------------------------
// GasInfo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_gas_price_returns_fixed_value() {
    let client = make_client(0);
    let gas_price = client.get_gas_price(None).await.unwrap();
    // Should return the fixed 100_000_000 yoctoNEAR gas price
    assert!(gas_price.to_balance() > 0);
}

// ---------------------------------------------------------------------------
// SendTx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transfer_native_token_returns_ok() {
    let client = make_client(0);
    let signer = test_signer();
    let receiver: AccountId = "receiver.near".parse().unwrap();
    let result = client
        .transfer_native_token(&signer, &receiver, NearToken::from_yoctonear(1))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn send_tx_returns_ok() {
    let client = make_client(0);
    let signer = test_signer();
    let receiver: AccountId = "receiver.near".parse().unwrap();
    let result = client.send_tx(&signer, &receiver, vec![]).await;
    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// exec_contract: non-swap methods
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exec_contract_non_swap_method_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(YoctoValue::from_yocto(
        BigDecimal::from(cash),
    ))));
    let client = make_client_with_portfolio(Arc::clone(&portfolio));
    let signer = test_signer();
    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();

    let result = client
        .exec_contract(
            &signer,
            &receiver,
            "storage_deposit",
            serde_json::json!({}),
            NearToken::from_yoctonear(1),
        )
        .await;
    assert!(result.is_ok());

    // Portfolio should be unchanged
    let state = portfolio.lock().await;
    assert_eq!(
        state.cash_balance,
        YoctoValue::from_yocto(BigDecimal::from(cash))
    );
    assert!(state.holdings.is_empty());
}

// ---------------------------------------------------------------------------
// exec_contract: swap parsing edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exec_contract_swap_empty_actions_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(YoctoValue::from_yocto(
        BigDecimal::from(cash),
    ))));
    let client = make_client_with_portfolio(Arc::clone(&portfolio));
    let signer = test_signer();
    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();

    let args = serde_json::json!({
        "actions": []
    });

    let result = client
        .exec_contract(
            &signer,
            &receiver,
            "swap",
            args,
            NearToken::from_yoctonear(1),
        )
        .await;
    assert!(result.is_ok());

    let state = portfolio.lock().await;
    assert_eq!(
        state.cash_balance,
        YoctoValue::from_yocto(BigDecimal::from(cash))
    );
    assert!(state.holdings.is_empty());
}

#[tokio::test]
async fn exec_contract_swap_no_actions_field_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(YoctoValue::from_yocto(
        BigDecimal::from(cash),
    ))));
    let client = make_client_with_portfolio(Arc::clone(&portfolio));
    let signer = test_signer();
    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();

    // "swap" method but no "actions" key
    let args = serde_json::json!({
        "some_other_key": "value"
    });

    let result = client
        .exec_contract(
            &signer,
            &receiver,
            "swap",
            args,
            NearToken::from_yoctonear(1),
        )
        .await;
    assert!(result.is_ok());

    let state = portfolio.lock().await;
    assert_eq!(
        state.cash_balance,
        YoctoValue::from_yocto(BigDecimal::from(cash))
    );
}

#[tokio::test]
async fn exec_contract_swap_zero_amount_in_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(YoctoValue::from_yocto(
        BigDecimal::from(cash),
    ))));
    let client = make_client_with_portfolio(Arc::clone(&portfolio));
    let signer = test_signer();
    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();

    let args = serde_json::json!({
        "actions": [{
            "pool_id": 1,
            "token_in": wnear_str(),
            "amount_in": U128(0),
            "token_out": "token-a.near",
            "min_amount_out": U128(0)
        }]
    });

    let result = client
        .exec_contract(
            &signer,
            &receiver,
            "swap",
            args,
            NearToken::from_yoctonear(1),
        )
        .await;
    assert!(result.is_ok());

    // amount_in=0 should skip swap execution
    let state = portfolio.lock().await;
    assert_eq!(
        state.cash_balance,
        YoctoValue::from_yocto(BigDecimal::from(cash))
    );
    assert!(state.holdings.is_empty());
}

#[tokio::test]
async fn exec_contract_swap_none_amount_in_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(YoctoValue::from_yocto(
        BigDecimal::from(cash),
    ))));
    let client = make_client_with_portfolio(Arc::clone(&portfolio));
    let signer = test_signer();
    let receiver: AccountId = "v2.ref-finance.near".parse().unwrap();

    let args = serde_json::json!({
        "actions": [{
            "pool_id": 1,
            "token_in": wnear_str(),
            "amount_in": null,
            "token_out": "token-a.near",
            "min_amount_out": U128(0)
        }]
    });

    let result = client
        .exec_contract(
            &signer,
            &receiver,
            "swap",
            args,
            NearToken::from_yoctonear(1),
        )
        .await;
    assert!(result.is_ok());

    // amount_in=None maps to 0 → skip
    let state = portfolio.lock().await;
    assert_eq!(
        state.cash_balance,
        YoctoValue::from_yocto(BigDecimal::from(cash))
    );
}

// ---------------------------------------------------------------------------
// estimate_swap_via_pools (pool-based swap calculation)
// ---------------------------------------------------------------------------

fn make_simple_pool(
    id: u32,
    token_a: &str,
    token_b: &str,
    amount_a: u128,
    amount_b: u128,
    total_fee: u32,
) -> std::sync::Arc<dex::PoolInfo> {
    use dex::{PoolInfo, PoolInfoBared};

    std::sync::Arc::new(PoolInfo::new(
        id,
        PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: vec![token_a.parse().unwrap(), token_b.parse().unwrap()],
            amounts: vec![U128(amount_a), U128(amount_b)],
            total_fee,
            shares_total_supply: U128(0),
            amp: 0,
        },
        chrono::Utc::now().naive_utc(),
    ))
}

#[test]
fn estimate_swap_single_hop_with_fee() {
    // Pool: wNEAR/USDT, 1000 NEAR liquidity, 5000 USDT liquidity, 0.3% fee
    let pool = make_simple_pool(
        1,
        "wrap.near",
        "usdt.tether-token.near",
        1_000_000_000_000_000_000_000_000_000, // 1000 NEAR (24 decimals)
        5_000_000_000,                         // 5000 USDT (6 decimals)
        30,                                    // 0.3% fee
    );
    let pools = dex::PoolInfoList::new(vec![pool]);

    let actions = vec![SwapAction {
        pool_id: 1,
        token_in: "wrap.near".parse().unwrap(),
        amount_in: Some(U128(1_000_000_000_000_000_000_000_000)), // 1 NEAR
        token_out: "usdt.tether-token.near".parse().unwrap(),
        min_amount_out: U128(0),
    }];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000_000_000_000_000_000_000);

    // With xy=k and 0.3% fee, output should be less than 5 USDT (no-fee rate)
    let output = result.unwrap();
    assert!(output > 0, "output should be positive");
    assert!(
        output < 5_000_000,
        "output should be less than 5 USDT (no-fee rate): got {output}"
    );
    // Rough check: 1 NEAR out of 1000 NEAR pool → ~0.1% of pool
    // Expected ~4.985 USDT (price impact + fee)
    assert!(
        output > 4_900_000,
        "output should be close to ~4.98 USDT: got {output}"
    );
}

#[test]
fn estimate_swap_multi_hop() {
    // Hop 1: wNEAR → tokenA (pool 1)
    // Hop 2: tokenA → tokenB (pool 2)
    let pool1 = make_simple_pool(
        1,
        "wrap.near",
        "token-a.near",
        1_000_000_000_000_000_000_000_000_000,  // 1000 NEAR
        10_000_000_000_000_000_000_000_000_000, // 10000 tokenA (24 decimals)
        30,
    );
    let pool2 = make_simple_pool(
        2,
        "token-a.near",
        "token-b.near",
        5_000_000_000_000_000_000_000_000_000, // 5000 tokenA
        2_000_000_000,                         // 2000 tokenB (6 decimals)
        30,
    );
    let pools = dex::PoolInfoList::new(vec![pool1, pool2]);

    let actions = vec![
        SwapAction {
            pool_id: 1,
            token_in: "wrap.near".parse().unwrap(),
            amount_in: Some(U128(1_000_000_000_000_000_000_000_000)), // 1 NEAR
            token_out: "token-a.near".parse().unwrap(),
            min_amount_out: U128(0),
        },
        SwapAction {
            pool_id: 2,
            token_in: "token-a.near".parse().unwrap(),
            amount_in: None, // uses output of previous hop
            token_out: "token-b.near".parse().unwrap(),
            min_amount_out: U128(0),
        },
    ];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000_000_000_000_000_000_000);
    let output = result.unwrap();
    // Should produce some tokenB, with fees deducted at each hop
    assert!(output > 0, "multi-hop output should be positive");
}

#[test]
fn estimate_swap_missing_pool_returns_none() {
    // Empty pool list → pool_id=1 not found
    let pools = dex::PoolInfoList::new(vec![]);

    let actions = vec![SwapAction {
        pool_id: 1,
        token_in: "wrap.near".parse().unwrap(),
        amount_in: Some(U128(1_000_000)),
        token_out: "usdt.tether-token.near".parse().unwrap(),
        min_amount_out: U128(0),
    }];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000);
    assert!(result.is_none(), "should return None when pool not found");
}

#[test]
fn estimate_swap_token_not_in_pool_returns_none() {
    // Pool has wNEAR/USDT but we try to swap wNEAR → tokenX
    let pool = make_simple_pool(
        1,
        "wrap.near",
        "usdt.tether-token.near",
        1_000_000_000_000_000_000_000_000_000,
        5_000_000_000,
        30,
    );
    let pools = dex::PoolInfoList::new(vec![pool]);

    let actions = vec![SwapAction {
        pool_id: 1,
        token_in: "wrap.near".parse().unwrap(),
        amount_in: Some(U128(1_000_000)),
        token_out: "unknown-token.near".parse().unwrap(), // not in pool
        min_amount_out: U128(0),
    }];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000);
    assert!(
        result.is_none(),
        "should return None when token not in pool"
    );
}

#[test]
fn estimate_swap_zero_liquidity_pool() {
    // Pool with zero liquidity in one side
    let pool = make_simple_pool(
        1,
        "wrap.near",
        "usdt.tether-token.near",
        0, // zero NEAR liquidity
        5_000_000_000,
        30,
    );
    let pools = dex::PoolInfoList::new(vec![pool]);

    let actions = vec![SwapAction {
        pool_id: 1,
        token_in: "wrap.near".parse().unwrap(),
        amount_in: Some(U128(1_000_000_000_000_000_000_000_000)),
        token_out: "usdt.tether-token.near".parse().unwrap(),
        min_amount_out: U128(0),
    }];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000_000_000_000_000_000_000);
    // Zero liquidity should return 0 or error, not panic
    if let Some(out) = result {
        assert_eq!(out, 0, "zero liquidity pool should return 0 output");
    }
}

#[test]
fn estimate_swap_zero_amount_in() {
    let pool = make_simple_pool(
        1,
        "wrap.near",
        "usdt.tether-token.near",
        1_000_000_000_000_000_000_000_000_000,
        5_000_000_000,
        30,
    );
    let pools = dex::PoolInfoList::new(vec![pool]);

    let actions = vec![SwapAction {
        pool_id: 1,
        token_in: "wrap.near".parse().unwrap(),
        amount_in: Some(U128(0)),
        token_out: "usdt.tether-token.near".parse().unwrap(),
        min_amount_out: U128(0),
    }];

    let result = estimate_swap_via_pools(&pools, &actions, 0);
    if let Some(out) = result {
        assert_eq!(out, 0, "zero input should produce zero output");
    }
}

// ---------------------------------------------------------------------------
// MockSentTx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_sent_tx_display() {
    let tx = MockSentTx { output_amount: 0 };
    assert_eq!(format!("{tx}"), "MockSentTx(sim, output=0)");

    let tx = MockSentTx {
        output_amount: 12345,
    };
    assert_eq!(format!("{tx}"), "MockSentTx(sim, output=12345)");
}

#[tokio::test]
async fn mock_sent_tx_wait_for_success_returns_ok() {
    use blockchain::jsonrpc::SentTx;
    let tx = MockSentTx {
        output_amount: 42000,
    };
    let result = tx.wait_for_success().await;
    assert!(result.is_ok());

    // Verify the output amount is encoded in the outcome
    let outcome = result.unwrap();
    if let near_primitives::views::FinalExecutionStatus::SuccessValue(val) = &outcome.status {
        let decoded: near_sdk::json_types::U128 = serde_json::from_slice(val).unwrap();
        assert_eq!(decoded.0, 42000);
    } else {
        panic!("expected SuccessValue status");
    }
}

// ---------------------------------------------------------------------------
// estimate_swap_via_pools: multi-hop mid-failure
// ---------------------------------------------------------------------------

#[test]
fn estimate_swap_multi_hop_second_pool_missing_returns_none() {
    // Hop 1: pool 1 exists (wNEAR → tokenA)
    // Hop 2: pool 2 does NOT exist → should return None
    let pool1 = make_simple_pool(
        1,
        "wrap.near",
        "token-a.near",
        1_000_000_000_000_000_000_000_000_000,  // 1000 NEAR
        10_000_000_000_000_000_000_000_000_000, // 10000 tokenA
        30,
    );
    let pools = dex::PoolInfoList::new(vec![pool1]); // only pool 1, no pool 2

    let actions = vec![
        SwapAction {
            pool_id: 1,
            token_in: "wrap.near".parse().unwrap(),
            amount_in: Some(U128(1_000_000_000_000_000_000_000_000)),
            token_out: "token-a.near".parse().unwrap(),
            min_amount_out: U128(0),
        },
        SwapAction {
            pool_id: 2, // does not exist
            token_in: "token-a.near".parse().unwrap(),
            amount_in: None,
            token_out: "token-b.near".parse().unwrap(),
            min_amount_out: U128(0),
        },
    ];

    let result = estimate_swap_via_pools(&pools, &actions, 1_000_000_000_000_000_000_000_000);
    assert!(
        result.is_none(),
        "should return None when second hop pool is missing"
    );
}
