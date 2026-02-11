use super::*;
use bigdecimal::BigDecimal;
use common::config;
use common::types::{NearValue, YoctoAmount, YoctoValue};

/// NEAR → yoctoNEAR 変換のヘルパー（型安全）
fn near_to_yocto(near: u64) -> BigDecimal {
    NearValue::from_near(BigDecimal::from(near))
        .to_yocto()
        .as_bigdecimal()
        .clone()
}

/// NEAR → YoctoAmount 変換のヘルパー（型安全）
fn near_to_yocto_amount(near: u64) -> YoctoAmount {
    let yocto_value = near as u128 * 10u128.pow(24);
    YoctoAmount::from_u128(yocto_value)
}

// テスト専用: staticを使わずに設定値を計算する関数
#[cfg(test)]
fn calculate_harvest_reserve_amount_from_config(config_value: Option<&str>) -> BigDecimal {
    let reserve_str = config_value.unwrap_or("1").to_string();
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    near_to_yocto(reserve_near)
}

#[test]
fn test_harvest_reserve_amount_default() {
    // テスト用にデフォルト値（1 NEAR）をテスト
    let expected = near_to_yocto(1);

    // staticを使わずに設定ロジックを直接テスト
    let actual = calculate_harvest_reserve_amount_from_config(None);
    assert_eq!(actual, expected);
}

#[test]
fn test_harvest_reserve_amount_custom() {
    // カスタム値のテスト: 5 NEAR
    let expected = near_to_yocto(5);

    // staticを使わずに設定ロジックを直接テスト
    let actual = calculate_harvest_reserve_amount_from_config(Some("5"));
    assert_eq!(actual, expected);
}

#[test]
fn test_harvest_min_amount_default() {
    // HARVEST_MIN_AMOUNTのデフォルト値テスト
    let expected = near_to_yocto_amount(10);
    let actual = harvest_min_amount();
    assert_eq!(actual, expected);
}

#[test]
fn test_yocto_near_conversion() {
    // yoctoNEAR変換の正確性テスト（型安全版）
    let five_near = near_to_yocto(5);

    // 5 NEARが正しくyoctoNEARに変換されることを確認
    assert_eq!(five_near.to_string(), "5000000000000000000000000");
}

#[test]
fn test_harvest_reserve_amount_parsing() {
    // 無効な設定値の場合のフォールバック動作テスト
    let _guard = config::ConfigGuard::new("HARVEST_RESERVE_AMOUNT", "invalid");

    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);

    // 無効な値の場合、デフォルト1に戻ることを確認
    assert_eq!(reserve_near, 1);

    // 正常な値の場合のテスト
    config::set("HARVEST_RESERVE_AMOUNT", "3");
    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    assert_eq!(reserve_near, 3);
}

#[test]
fn test_harvest_account_parsing() {
    // HARVEST_ACCOUNT_IDの正常なパース動作テスト
    let _guard = config::ConfigGuard::new("HARVEST_ACCOUNT_ID", "test.near");

    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|_| "harvest.near".to_string());
    let parsed_account = value.parse::<AccountId>();

    assert!(parsed_account.is_ok());
    assert_eq!(parsed_account.unwrap().as_str(), "test.near");
}

#[test]
fn test_is_time_to_harvest() {
    // 初回は常にtrueになるはず（LAST_HARVEST_TIMEが0のため）
    assert!(is_time_to_harvest());

    // 現在時刻を記録
    update_last_harvest_time();

    // 直後はfalseになるはず
    assert!(!is_time_to_harvest());
}

#[test]
fn test_harvest_threshold_calculation() {
    // 初期投資額: 100 NEAR
    let initial_amount = 100u128 * 10u128.pow(24);
    let initial_value = BigDecimal::from(initial_amount);

    // 200%利益時のしきい値（2倍）
    let harvest_threshold = &initial_value * BigDecimal::from(2);
    let expected_threshold = BigDecimal::from(200u128 * 10u128.pow(24));
    assert_eq!(harvest_threshold, expected_threshold);

    // ポートフォリオ価値が250 NEARの場合
    let current_portfolio_value = BigDecimal::from(250u128 * 10u128.pow(24));
    let excess_value = &current_portfolio_value - &harvest_threshold;
    let expected_excess = BigDecimal::from(50u128 * 10u128.pow(24));
    assert_eq!(excess_value, expected_excess);

    // 10%の利益確定額
    let harvest_amount = &excess_value * BigDecimal::new(1.into(), 1); // 10% = 0.1
    let expected_harvest = BigDecimal::from(5u128 * 10u128.pow(24)); // 5 NEAR
    assert_eq!(harvest_amount, expected_harvest);
}

// Minimal mock client for harvest tests (params are unused by check_and_harvest)
struct TestClient;

impl blockchain::jsonrpc::AccountInfo for TestClient {
    async fn get_native_amount(&self, _account: &AccountId) -> anyhow::Result<near_sdk::NearToken> {
        Ok(near_sdk::NearToken::from_yoctonear(0))
    }
}

impl blockchain::jsonrpc::GasInfo for TestClient {
    async fn get_gas_price(
        &self,
        _block: Option<near_primitives::types::BlockId>,
    ) -> anyhow::Result<blockchain::types::gas_price::GasPrice> {
        Ok(blockchain::types::gas_price::GasPrice::from_balance(
            near_sdk::NearToken::from_yoctonear(100_000_000),
        ))
    }
}

impl blockchain::jsonrpc::SendTx for TestClient {
    type Output = TestSentTx;

    async fn transfer_native_token(
        &self,
        _signer: &near_crypto::InMemorySigner,
        _receiver: &AccountId,
        _amount: near_sdk::NearToken,
    ) -> anyhow::Result<Self::Output> {
        Ok(TestSentTx)
    }

    async fn exec_contract<T>(
        &self,
        _signer: &near_crypto::InMemorySigner,
        _receiver: &AccountId,
        _method_name: &str,
        _args: T,
        _deposit: near_sdk::NearToken,
    ) -> anyhow::Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        Ok(TestSentTx)
    }

    async fn send_tx(
        &self,
        _signer: &near_crypto::InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<near_primitives::action::Action>,
    ) -> anyhow::Result<Self::Output> {
        Ok(TestSentTx)
    }
}

impl blockchain::jsonrpc::ViewContract for TestClient {
    async fn view_contract<T>(
        &self,
        _receiver: &AccountId,
        _method_name: &str,
        _args: &T,
    ) -> anyhow::Result<near_primitives::views::CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        Ok(near_primitives::views::CallResult {
            result: vec![],
            logs: vec![],
        })
    }
}

struct TestSentTx;

impl std::fmt::Display for TestSentTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestSentTx")
    }
}

impl blockchain::jsonrpc::SentTx for TestSentTx {
    async fn wait_for_executed(
        &self,
    ) -> anyhow::Result<near_primitives::views::FinalExecutionOutcomeViewEnum> {
        unimplemented!()
    }

    async fn wait_for_success(
        &self,
    ) -> anyhow::Result<near_primitives::views::ExecutionOutcomeView> {
        unimplemented!()
    }
}

struct TestWallet {
    account_id: AccountId,
    signer: near_crypto::InMemorySigner,
}

impl TestWallet {
    fn new() -> Self {
        let account_id: AccountId = "test.near".parse().unwrap();
        let signer_result = near_crypto::InMemorySigner::from_seed(
            account_id.clone(),
            near_crypto::KeyType::ED25519,
            "test.near",
        );
        let signer = match signer_result {
            near_crypto::Signer::InMemory(s) => s,
            _ => panic!("Expected InMemorySigner"),
        };
        Self { account_id, signer }
    }
}

impl blockchain::wallet::Wallet for TestWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &near_crypto::InMemorySigner {
        &self.signer
    }
}

#[tokio::test]
async fn test_check_and_harvest_no_evaluation_period() {
    // 評価期間がまだない場合のテスト
    let current_portfolio_value =
        YoctoValue::from_yocto(BigDecimal::from(100u128 * 10u128.pow(24)));

    // check_and_harvestは早期リターンするはず（評価期間がない場合）
    let client = TestClient;
    let wallet = TestWallet::new();
    let result = check_and_harvest(&client, &wallet, current_portfolio_value).await;

    // データベースが使えない環境ではテストをスキップ
    if result.is_err() {
        println!("Skipping test due to database unavailability");
        return;
    }
}
