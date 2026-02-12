use super::*;
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, ViewContract};
use near_crypto::InMemorySigner;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use std::collections::BTreeMap;

fn make_client_with_decimals(
    cash: u128,
    holdings: BTreeMap<String, u128>,
    decimals: Vec<(&str, u8)>,
) -> SimulationClient {
    let mut state = PortfolioState::new(cash);
    state.holdings = holdings;
    for (token, dec) in decimals {
        state.decimals.insert(token.to_string(), dec);
    }
    let portfolio = Arc::new(Mutex::new(state));
    SimulationClient::new(portfolio, cash)
}

fn make_client(cash: u128, holdings: BTreeMap<String, u128>) -> SimulationClient {
    make_client_with_decimals(cash, holdings, vec![])
}

fn make_client_with_portfolio(portfolio: Arc<Mutex<PortfolioState>>) -> SimulationClient {
    SimulationClient::new(portfolio, 0)
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
    let mut holdings = BTreeMap::new();
    holdings.insert("usdt.tether-token.near".to_string(), 1_000_000u128);

    let cash = 50_000_000_000_000_000_000_000_000u128; // 50 NEAR
    let client = make_client_with_decimals(cash, holdings, vec![("usdt.tether-token.near", 6)]);

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
    let client = make_client(0, BTreeMap::new());

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
    let mut holdings = BTreeMap::new();
    holdings.insert("usdt.tether-token.near".to_string(), 1_000_000u128);

    let client = make_client_with_decimals(0, holdings, vec![("usdt.tether-token.near", 6)]);

    let receiver: AccountId = "usdt.tether-token.near".parse().unwrap();
    let result = client
        .view_contract(&receiver, "ft_metadata", &serde_json::json!({}))
        .await
        .unwrap();

    let metadata: serde_json::Value = serde_json::from_slice(&result.result).unwrap();
    // decimals should come from portfolio state (6 for usdt)
    assert_eq!(metadata["decimals"], 6);
}

#[tokio::test]
async fn view_contract_ft_balance_of_returns_large_value() {
    let client = make_client(0, BTreeMap::new());

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
    let client = make_client(0, BTreeMap::new());

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
    let client = make_client(0, BTreeMap::new());

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
    let client = make_client(initial, BTreeMap::new());

    let account: AccountId = "sim.near".parse().unwrap();
    let amount = client.get_native_amount(&account).await.unwrap();
    assert_eq!(amount.as_yoctonear(), initial);
}

// ---------------------------------------------------------------------------
// GasInfo
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_gas_price_returns_fixed_value() {
    let client = make_client(0, BTreeMap::new());
    let gas_price = client.get_gas_price(None).await.unwrap();
    // Should return the fixed 100_000_000 yoctoNEAR gas price
    assert!(gas_price.to_balance() > 0);
}

// ---------------------------------------------------------------------------
// SendTx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn transfer_native_token_returns_ok() {
    let client = make_client(0, BTreeMap::new());
    let signer = test_signer();
    let receiver: AccountId = "receiver.near".parse().unwrap();
    let result = client
        .transfer_native_token(&signer, &receiver, NearToken::from_yoctonear(1))
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn send_tx_returns_ok() {
    let client = make_client(0, BTreeMap::new());
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
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(cash)));
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
    assert_eq!(state.cash_balance, cash);
    assert!(state.holdings.is_empty());
}

// ---------------------------------------------------------------------------
// exec_contract: swap parsing edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn exec_contract_swap_empty_actions_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(cash)));
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
    assert_eq!(state.cash_balance, cash);
    assert!(state.holdings.is_empty());
}

#[tokio::test]
async fn exec_contract_swap_no_actions_field_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(cash)));
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
    assert_eq!(state.cash_balance, cash);
}

#[tokio::test]
async fn exec_contract_swap_zero_amount_in_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(cash)));
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
    assert_eq!(state.cash_balance, cash);
    assert!(state.holdings.is_empty());
}

#[tokio::test]
async fn exec_contract_swap_none_amount_in_is_noop() {
    let cash = 100_000_000_000_000_000_000_000_000u128;
    let portfolio = Arc::new(Mutex::new(PortfolioState::new(cash)));
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
    assert_eq!(state.cash_balance, cash);
}

// ---------------------------------------------------------------------------
// MockSentTx
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_sent_tx_display() {
    let tx = MockSentTx;
    assert_eq!(format!("{tx}"), "MockSentTx(sim)");
}

#[tokio::test]
async fn mock_sent_tx_wait_for_success_returns_ok() {
    use blockchain::jsonrpc::SentTx;
    let tx = MockSentTx;
    let result = tx.wait_for_success().await;
    assert!(result.is_ok());
}
