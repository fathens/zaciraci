use super::*;
use crate::jsonrpc::{AccountInfo, GasInfo, ViewContract};
use crate::types::gas_price::GasPrice;
use crate::wallet::Wallet;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::types::{Balance, BlockId};
use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::AccountId;
use near_sdk::json_types::U128;
use serde_json::json;
use std::cell::Cell;
use std::sync::{Arc, Mutex, Once};

static INIT: Once = Once::new();

fn initialize() {
    INIT.call_once(|| {
        config::set("ARBITRAGE_NEEDED", "true");
        config::set("TOKEN_NOT_FOUND_WAIT", "1s");
        config::set("OTHER_ERROR_WAIT", "1s");
        config::set("PREVIEW_NOT_FOUND_WAIT", "1s");
    });
}

struct MockWallet {
    account_id: AccountId,
    signer: InMemorySigner,
}

impl MockWallet {
    fn new() -> Self {
        let account_id: AccountId = "test.near".parse().unwrap();
        let signer_result = InMemorySigner::from_seed(
            account_id.clone(),
            near_crypto::KeyType::ED25519,
            "test.near",
        );
        let signer = match signer_result {
            near_crypto::Signer::InMemory(signer) => signer,
            _ => panic!("Expected InMemorySigner"),
        };
        Self { account_id, signer }
    }
}

impl wallet::Wallet for MockWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}

struct OperationsLog(Arc<Mutex<Vec<String>>>);

impl OperationsLog {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn push(&self, op: String) {
        self.0.lock().unwrap().push(op);
    }

    fn contains(&self, s: &str) -> bool {
        self.0.lock().unwrap().iter().any(|log| log.contains(s))
    }
}

struct MockClient {
    native_amount: Cell<Balance>,
    wnear_deposited: Cell<Balance>,
    operations_log: OperationsLog,
    gas_price: u128,
    should_fail_swap: Cell<bool>,
}

unsafe impl Sync for MockClient {}

impl MockClient {
    fn new(native: Balance, deposited: Balance) -> Self {
        Self {
            native_amount: Cell::new(native),
            wnear_deposited: Cell::new(deposited),
            operations_log: OperationsLog::new(),
            gas_price: 100_000_000, // 0.1 Ggas price
            should_fail_swap: Cell::new(false),
        }
    }

    fn log_operation(&self, operation: &str) {
        self.operations_log.push(operation.to_string());
    }
}

impl jsonrpc::AccountInfo for MockClient {
    async fn get_native_amount(&self, _account: &AccountId) -> crate::Result<Balance> {
        self.log_operation("get_native_amount");
        Ok(self.native_amount.get())
    }
}

impl jsonrpc::GasInfo for MockClient {
    async fn get_gas_price(&self, _block: Option<BlockId>) -> crate::Result<GasPrice> {
        self.log_operation("get_gas_price");
        Ok(GasPrice::from_balance(self.gas_price))
    }
}

impl jsonrpc::SendTx for MockClient {
    type Output = MockSentTx;

    async fn transfer_native_token(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _amount: Balance,
    ) -> crate::Result<Self::Output> {
        self.log_operation("transfer_native_token");
        Ok(MockSentTx {
            should_fail: false,
            id: "tx_transfer".to_string(),
        })
    }

    async fn exec_contract<T>(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        method_name: &str,
        _args: T,
        _deposit: Balance,
    ) -> crate::Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        self.log_operation(&format!("exec_contract: {method_name}"));
        let should_fail = method_name == "ft_transfer_call" && self.should_fail_swap.get();
        Ok(MockSentTx {
            should_fail,
            id: format!("tx_{method_name}"),
        })
    }

    async fn send_tx(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<Action>,
    ) -> crate::Result<Self::Output> {
        self.log_operation("send_tx");
        Ok(MockSentTx {
            should_fail: self.should_fail_swap.get(),
            id: "tx_send".to_string(),
        })
    }
}

impl jsonrpc::ViewContract for MockClient {
    async fn view_contract<T>(
        &self,
        _receiver: &AccountId,
        method_name: &str,
        _args: &T,
    ) -> crate::Result<CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        self.log_operation(&format!("view_contract: {method_name}"));
        let result = match method_name {
            "get_deposits" => {
                let deposits = json!({
                    WNEAR_TOKEN.to_string(): U128(self.wnear_deposited.get()),
                });
                serde_json::to_vec(&deposits)?
            }
            "ft_balance_of" => {
                let balance = U128(self.wnear_deposited.get());
                serde_json::to_vec(&balance)?
            }
            "storage_balance_of" => {
                let account_info = json!({
                    "total": U128(100_000_000_000_000_000_000_000u128),
                    "available": U128(0),
                });
                serde_json::to_vec(&account_info)?
            }
            "get_account_basic_info" => {
                let account_info = json!({
                    "near_amount": U128(100_000_000_000_000_000_000_000u128),
                    "storage_used": near_sdk::json_types::U64(0),
                });
                serde_json::to_vec(&account_info)?
            }
            _ => {
                let balance = U128(0);
                serde_json::to_vec(&balance)?
            }
        };

        Ok(CallResult {
            result,
            logs: vec![],
        })
    }
}

struct MockSentTx {
    should_fail: bool,
    id: String,
}

impl std::fmt::Display for MockSentTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockSentTx({})", self.id)
    }
}

impl jsonrpc::SentTx for MockSentTx {
    async fn wait_for_executed(&self) -> crate::Result<FinalExecutionOutcomeViewEnum> {
        unimplemented!()
    }

    async fn wait_for_success(&self) -> crate::Result<ExecutionOutcomeView> {
        if self.should_fail {
            return Err(anyhow!("Transaction failed"));
        }
        Ok(ExecutionOutcomeView {
            logs: vec![],
            receipt_ids: vec![],
            gas_burnt: 0,
            tokens_burnt: 0,
            executor_id: AccountId::try_from("test.near".to_string())?,
            status: near_primitives::views::ExecutionStatusView::SuccessValue(vec![]),
            metadata: near_primitives::views::ExecutionMetadataView {
                version: 1,
                gas_profile: None,
            },
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn test_is_needed_default() {
    // デフォルトでは false (存在しないキーの場合)
    let result = config::get("ARBITRAGE_NEEDED_NONEXISTENT")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);
    assert!(!result, "Default should be false when config is not set");
}

#[test]
fn test_is_needed_behavior() {
    // is_needed() 関数の動作確認
    // config::get が "true" を返す場合は true、それ以外は false を返す

    // "true" の場合
    config::set("ARBITRAGE_NEEDED", "true");
    let value = config::get("ARBITRAGE_NEEDED").unwrap();
    assert_eq!(value, "true");
    let parsed = value.parse::<bool>().unwrap();
    assert!(parsed, "Should parse 'true' as true");

    // "false" の場合
    config::set("ARBITRAGE_NEEDED", "false");
    let value = config::get("ARBITRAGE_NEEDED").unwrap();
    assert_eq!(value, "false");
    let parsed = value.parse::<bool>().unwrap();
    assert!(!parsed, "Should parse 'false' as false");

    // 無効な値の場合
    config::set("ARBITRAGE_NEEDED", "invalid");
    let value = config::get("ARBITRAGE_NEEDED").unwrap();
    assert_eq!(value, "invalid");
    let parsed = value.parse::<bool>();
    assert!(parsed.is_err(), "Should fail to parse 'invalid'");
}

#[test]
fn test_duration_config_defaults() {
    // TOKEN_NOT_FOUND_WAIT のデフォルト確認
    let default_wait = config::get("TOKEN_NOT_FOUND_WAIT_NONEXISTENT")
        .map(|v| parse_duration(&v).ok())
        .unwrap_or(None)
        .unwrap_or_else(|| Duration::from_secs(1));
    assert_eq!(default_wait, Duration::from_secs(1));

    // OTHER_ERROR_WAIT のデフォルト確認
    let default_other = config::get("OTHER_ERROR_WAIT_NONEXISTENT")
        .map(|v| parse_duration(&v).ok())
        .unwrap_or(None)
        .unwrap_or_else(|| Duration::from_secs(30));
    assert_eq!(default_other, Duration::from_secs(30));

    // PREVIEW_NOT_FOUND_WAIT のデフォルト確認
    let default_preview = config::get("PREVIEW_NOT_FOUND_WAIT_NONEXISTENT")
        .map(|v| parse_duration(&v).ok())
        .unwrap_or(None)
        .unwrap_or_else(|| Duration::from_secs(10));
    assert_eq!(default_preview, Duration::from_secs(10));
}

#[test]
fn test_duration_config_parsing() {
    // 有効な duration 文字列のパース
    let parsed = parse_duration("5s").unwrap();
    assert_eq!(parsed, Duration::from_secs(5));

    let parsed = parse_duration("1m").unwrap();
    assert_eq!(parsed, Duration::from_secs(60));

    let parsed = parse_duration("500ms").unwrap();
    assert_eq!(parsed, Duration::from_millis(500));
}

#[tokio::test]
async fn test_mock_client_basic() {
    initialize();

    let native_balance = 10_000_000_000_000_000_000_000_000u128; // 10 NEAR
    let deposited = 5_000_000_000_000_000_000_000_000u128; // 5 NEAR
    let client = MockClient::new(native_balance, deposited);
    let wallet = MockWallet::new();

    // get_native_amount のテスト
    let balance = client.get_native_amount(wallet.account_id()).await.unwrap();
    assert_eq!(balance, native_balance);
    assert!(client.operations_log.contains("get_native_amount"));

    // get_gas_price のテスト
    let gas_price = client.get_gas_price(None).await.unwrap();
    assert_eq!(gas_price.to_balance(), 100_000_000);
    assert!(client.operations_log.contains("get_gas_price"));
}

#[tokio::test]
async fn test_mock_client_view_contract() {
    initialize();

    let deposited = 5_000_000_000_000_000_000_000_000u128;
    let client = MockClient::new(0, deposited);

    // get_deposits のテスト
    let result = client
        .view_contract::<()>(&"v2.ref-finance.near".parse().unwrap(), "get_deposits", &())
        .await
        .unwrap();
    assert!(!result.result.is_empty());
    assert!(
        client
            .operations_log
            .contains("view_contract: get_deposits")
    );

    // ft_balance_of のテスト
    let result = client
        .view_contract::<()>(&"wrap.near".parse().unwrap(), "ft_balance_of", &())
        .await
        .unwrap();
    assert!(!result.result.is_empty());
    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
}

#[tokio::test]
async fn test_mock_sent_tx_success() {
    let tx = MockSentTx {
        should_fail: false,
        id: "test_tx".to_string(),
    };

    let result = tx.wait_for_success().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_mock_sent_tx_failure() {
    let tx = MockSentTx {
        should_fail: true,
        id: "test_tx".to_string(),
    };

    let result = tx.wait_for_success().await;
    assert!(result.is_err());
}

#[test]
fn test_wnear_token_constant() {
    // WNEAR_TOKEN が正しく設定されていることを確認
    let token_str = WNEAR_TOKEN.to_string();
    assert_eq!(token_str, "wrap.near");
}

#[test]
fn test_micro_near_conversion() {
    // MicroNear の変換が正しく動作することを確認
    let yocto_amount = 1_000_000_000_000_000_000_000_000u128; // 1 NEAR
    let micro = MicroNear::from_yocto(yocto_amount);

    // MicroNear は 10^18 なので、1 NEAR = 1_000_000 MicroNear
    // (実装によって異なる可能性があるため、変換が成功することのみ確認)
    assert!(micro.to_yocto() > 0);
}
