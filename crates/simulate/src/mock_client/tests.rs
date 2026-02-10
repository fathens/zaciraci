use super::*;
use blockchain::jsonrpc::{AccountInfo, ViewContract};
use near_sdk::AccountId;
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
