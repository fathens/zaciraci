use super::*;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::NearToken;
use near_sdk::json_types::U128;
use serde_json::json;
use serial_test::serial;
use std::cell::Cell;
use std::sync::{Arc, Mutex, Once};

static INIT: Once = Once::new();

// Test constant for storage deposit amount
const DEFAULT_DEPOSIT: u128 = 100_000_000_000_000_000_000_000; // 0.1 NEAR

// Helper to get MINIMUM_NATIVE_BALANCE as u128 for tests
fn min_native_balance() -> u128 {
    MINIMUM_NATIVE_BALANCE.as_yoctonear()
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

impl Wallet for MockWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}

fn initialize() {
    INIT.call_once(|| {
        config::set("HARVEST_ACCOUNT_ID", "harvest.near");
        config::set("TRADE_ACCOUNT_RESERVE", "1");
    });
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
    native_amount: Cell<u128>,
    wnear_amount: Cell<u128>,
    wnear_deposited: Cell<u128>,
    operations_log: OperationsLog,
    should_fail_near_deposit: Cell<bool>,
    should_fail_ft_transfer: Cell<bool>,
}

unsafe impl Sync for MockClient {}

impl MockClient {
    fn new(native: u128, wnear: u128, deposited: u128) -> Self {
        Self {
            native_amount: Cell::new(native),
            wnear_amount: Cell::new(wnear),
            wnear_deposited: Cell::new(deposited),
            operations_log: OperationsLog::new(),
            should_fail_near_deposit: Cell::new(false),
            should_fail_ft_transfer: Cell::new(false),
        }
    }

    fn set_near_deposit_failure(&self, should_fail: bool) {
        self.should_fail_near_deposit.set(should_fail);
    }

    fn set_ft_transfer_failure(&self, should_fail: bool) {
        self.should_fail_ft_transfer.set(should_fail);
    }

    fn log_operation(&self, operation: &str) {
        self.operations_log.push(operation.to_string());
    }
}

impl AccountInfo for MockClient {
    async fn get_native_amount(&self, _account: &AccountId) -> Result<NearToken> {
        self.log_operation("get_native_amount");
        Ok(NearToken::from_yoctonear(self.native_amount.get()))
    }
}

impl SendTx for MockClient {
    type Output = MockSentTx;

    async fn transfer_native_token(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _amount: NearToken,
    ) -> Result<Self::Output> {
        self.log_operation("transfer_native_token");
        Ok(MockSentTx { should_fail: false })
    }

    async fn exec_contract<T>(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        method_name: &str,
        args: T,
        deposit: NearToken,
    ) -> Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        self.log_operation(&format!("exec_contract: {method_name}"));
        let deposit_yocto = deposit.as_yoctonear();
        let should_fail = match method_name {
            "near_deposit" => {
                if !self.should_fail_near_deposit.get() {
                    self.native_amount
                        .set(self.native_amount.get().saturating_sub(deposit_yocto));
                    self.wnear_amount
                        .set(self.wnear_amount.get().saturating_add(deposit_yocto));
                }
                self.should_fail_near_deposit.get()
            }
            "ft_transfer_call" => {
                if !self.should_fail_ft_transfer.get() {
                    let args_str = serde_json::to_string(&args)?;
                    let args_value: serde_json::Value = serde_json::from_str(&args_str)?;
                    let amount: u128 = args_value["amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default();
                    self.wnear_amount
                        .set(self.wnear_amount.get().saturating_sub(amount));
                    self.wnear_deposited
                        .set(self.wnear_deposited.get().saturating_add(amount));
                }
                self.should_fail_ft_transfer.get()
            }
            "withdraw" => {
                let args_str = serde_json::to_string(&args)?;
                let args_value: serde_json::Value = serde_json::from_str(&args_str)?;
                let amount: u128 = args_value["amount"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default();
                self.native_amount
                    .set(self.native_amount.get().saturating_add(amount));
                self.wnear_deposited
                    .set(self.wnear_deposited.get().saturating_sub(amount));
                false
            }
            _ => false,
        };
        Ok(MockSentTx { should_fail })
    }

    async fn send_tx(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<Action>,
    ) -> Result<Self::Output> {
        self.log_operation("send_tx");
        Ok(MockSentTx { should_fail: false })
    }
}

impl ViewContract for MockClient {
    async fn view_contract<T>(
        &self,
        _receiver: &AccountId,
        method_name: &str,
        _args: &T,
    ) -> Result<CallResult>
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
                let balance = U128(self.wnear_amount.get());
                serde_json::to_vec(&balance)?
            }
            "storage_balance_of" => {
                let account_info = json!({
                    "total": U128(DEFAULT_DEPOSIT),
                    "available": U128(0),
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
}

impl SentTx for MockSentTx {
    async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum> {
        unimplemented!()
    }

    async fn wait_for_success(&self) -> Result<ExecutionOutcomeView> {
        if self.should_fail {
            return Err(anyhow!("Transaction failed"));
        }
        Ok(ExecutionOutcomeView {
            logs: vec![],
            receipt_ids: vec![],
            gas_burnt: near_primitives::types::Gas::from_gas(0),
            tokens_burnt: NearToken::from_yoctonear(0),
            executor_id: AccountId::try_from("test.near".to_string())?,
            status: near_primitives::views::ExecutionStatusView::SuccessValue(vec![]),
            metadata: near_primitives::views::ExecutionMetadataView {
                version: 1,
                gas_profile: None,
            },
        })
    }
}

#[tokio::test]
async fn test_start() {
    initialize();

    let required_balance = DEFAULT_REQUIRED_BALANCE.as_yoctonear();
    let client = MockClient::new(required_balance << 5, 0, 0);
    let wallet = MockWallet::new();

    let result = start(&client, &wallet, &WNEAR_TOKEN, None).await;
    let balance = result.unwrap();
    // refill が実行されるため、refill後の残高が返される
    assert_eq!(balance.as_yoctonear(), required_balance);

    assert!(
        client
            .operations_log
            .contains("view_contract: get_deposits")
    );
}

#[tokio::test]
#[serial(harvest)]
async fn test_harvest_with_sufficient_balance() {
    initialize();

    let required = 1_000_000u128;
    let native_balance = required << 5;
    let client = MockClient::new(native_balance, 0, 0);
    let wallet = MockWallet::new();

    LAST_HARVEST.store(0, Ordering::Relaxed);

    let result = harvest(
        &client,
        &wallet,
        &WNEAR_TOKEN,
        NearToken::from_yoctonear(1000),
        NearToken::from_yoctonear(required),
    )
    .await;
    assert!(result.is_ok());

    assert!(client.operations_log.contains("get_native_amount"));
}

#[tokio::test]
async fn test_harvest_with_insufficient_balance() {
    initialize();

    let required = 1_000_000u128;
    let native_balance = required << 3;
    let client = MockClient::new(native_balance, 0, 0);
    let wallet = MockWallet::new();

    let result = harvest(
        &client,
        &wallet,
        &WNEAR_TOKEN,
        NearToken::from_yoctonear(1000),
        NearToken::from_yoctonear(required),
    )
    .await;
    assert!(result.is_ok());

    assert!(client.operations_log.contains("get_native_amount"));
}

#[test]
#[serial(harvest)]
fn test_is_time_to_harvest() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST.store(now - harvest_interval() - 1, Ordering::Relaxed);
    assert!(is_time_to_harvest());
    LAST_HARVEST.store(now - harvest_interval(), Ordering::Relaxed);
    assert!(!is_time_to_harvest());
    LAST_HARVEST.store(now - harvest_interval() + 1, Ordering::Relaxed);
    assert!(!is_time_to_harvest());
    LAST_HARVEST.store(now - harvest_interval() + 2, Ordering::Relaxed);
    assert!(!is_time_to_harvest());
}

#[test]
#[serial(harvest)]
fn test_update_last_harvest() {
    LAST_HARVEST.store(0, Ordering::Relaxed);
    update_last_harvest();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert_eq!(now, LAST_HARVEST.load(Ordering::Relaxed));
}

#[tokio::test]
async fn test_refill_with_sufficient_wrapped_balance() {
    initialize();

    let want = 1_000_000u128;
    let client = MockClient::new(want << 1, want << 1, want << 1);
    let wallet = MockWallet::new();

    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_with_sufficient_native_balance() {
    initialize();

    let want = NearToken::from_near(2).as_yoctonear();
    let client = MockClient::new(want << 2, 0, 0);
    let wallet = MockWallet::new();

    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_with_insufficient_native_balance() {
    initialize();

    let want = 1_000_000u128;
    let client = MockClient::new(min_native_balance(), 0, 0);
    let wallet = MockWallet::new();

    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(!client.operations_log.contains("near_deposit"));
    assert!(!client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_scenarios() {
    initialize();
    let want = NearToken::from_near(2).as_yoctonear();

    // Case 1: Wrapped残高が十分ある
    let client = MockClient::new(want + min_native_balance(), want, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(!client.operations_log.contains("near_deposit"));

    // Case 2: Native残高が十分ある
    let client = MockClient::new(want * 2 + min_native_balance(), 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_edge_cases() {
    initialize();
    let wallet = MockWallet::new();

    // Case 1: want値が0の場合
    let client = MockClient::new(min_native_balance(), 0, 0);
    let result = refill(&client, &wallet, NearToken::from_yoctonear(0)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(!client.operations_log.contains("near_deposit"));
    assert!(!client.operations_log.contains("ft_transfer_call"));

    // Case 2: want値が非常に大きい場合
    let want = u128::MAX;
    let client = MockClient::new(want, 0, 0);
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));

    // Case 3: ネイティブ残高がちょうど MINIMUM_NATIVE_BALANCE の場合
    let want = 1_000_000u128;
    let client = MockClient::new(min_native_balance(), 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(client.operations_log.contains("get_native_amount"));

    // Case 4: MINIMUM_NATIVE_BALANCEより少し多いnative残高
    let client = MockClient::new(min_native_balance() + 1, 0, 0);
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("exec_contract: near_deposit")
    );
}

#[tokio::test]
async fn test_refill_transaction_order() {
    initialize();

    let want = 1_000_000u128;
    let client = MockClient::new(want * 2 + min_native_balance(), 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    // 操作の順序を確認
    let binding = client.operations_log.0.lock().unwrap();
    let operations: Vec<_> = binding.iter().collect();
    assert!(operations.len() >= 3, "Should have at least 3 operations");

    // ft_balance_of が最初に呼ばれることを確認
    let ft_balance_of_idx = operations
        .iter()
        .position(|op| op.contains("view_contract: ft_balance_of"))
        .expect("ft_balance_of should be called");
    assert_eq!(
        ft_balance_of_idx, 0,
        "ft_balance_of should be the first operation"
    );

    // near_deposit が ft_transfer_call の前に呼ばれることを確認
    let near_deposit_idx = operations
        .iter()
        .position(|op| op.contains("near_deposit"))
        .expect("near_deposit should be called");
    let ft_transfer_idx = operations
        .iter()
        .position(|op| op.contains("ft_transfer_call"))
        .expect("ft_transfer_call should be called");
    assert!(
        near_deposit_idx < ft_transfer_idx,
        "near_deposit should be called before ft_transfer_call"
    );
}

#[tokio::test]
async fn test_refill_error_recovery() {
    initialize();

    let want = 1_000_000u128;
    let client = MockClient::new(want * 2 + min_native_balance(), 0, 0);
    client.set_near_deposit_failure(true);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_err());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(!client.operations_log.contains("ft_transfer_call"));

    let client = MockClient::new(want * 2 + min_native_balance(), 0, 0);
    client.set_ft_transfer_failure(true);
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_err());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_boundary_conditions() {
    initialize();

    // Case 1: want値がMINIMUM_NATIVE_BALANCEと同じ
    let client = MockClient::new(min_native_balance() * 3, 0, 0);
    let wallet = MockWallet::new();
    let result = refill(
        &client,
        &wallet,
        NearToken::from_yoctonear(min_native_balance()),
    )
    .await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));

    // Case 2: Native残高がwant + MINIMUM_NATIVE_BALANCEちょうど
    let want = 1_000_000u128;
    let client = MockClient::new(want + min_native_balance(), 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_overflow_conditions() {
    initialize();

    // Case 1: want値が非常に大きく、Native残高との加算でオーバーフローする可能性
    let want = u128::MAX - min_native_balance() + 1;
    let client = MockClient::new(u128::MAX, 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));

    // Case 2: Native残高とWrapped残高の合計が要求額に満たない
    let want = 1_000_000u128;
    let native = want / 2 + min_native_balance();
    let wrapped = want / 4;
    let client = MockClient::new(native, wrapped, wrapped);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("near_deposit"));
    assert!(client.operations_log.contains("ft_transfer_call"));
}

#[tokio::test]
async fn test_refill_combined_balances() {
    initialize();

    // Case 1: wrapped残高とnative残高の合計が要求額を満たす
    let want = 1_000_000u128;
    let wrapped = want / 2;
    let native = want / 2 + min_native_balance();
    let client = MockClient::new(native, wrapped, wrapped);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("view_contract: ft_balance_of")
    );
    assert!(client.operations_log.contains("get_native_amount"));

    // Case 2: wrapped残高が一部あり、native残高から補充
    let want = 1_000_000u128;
    let wrapped = want / 4;
    let native = want + min_native_balance();
    let client = MockClient::new(native, wrapped, wrapped);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("exec_contract: near_deposit")
    );
    assert!(
        client
            .operations_log
            .contains("exec_contract: ft_transfer_call")
    );
}

#[tokio::test]
async fn test_refill_minimum_balances() {
    initialize();

    // Case 1: ちょうどMINIMUM_NATIVE_BALANCEのnative残高
    let want = 1_000u128;
    let client = MockClient::new(min_native_balance(), 0, 0);
    let wallet = MockWallet::new();
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(client.operations_log.contains("get_native_amount"));

    // Case 2: MINIMUM_NATIVE_BALANCEより少し多いnative残高
    let client = MockClient::new(min_native_balance() + 1, 0, 0);
    let result = refill(&client, &wallet, NearToken::from_yoctonear(want)).await;
    assert!(result.is_ok());

    assert!(
        client
            .operations_log
            .contains("exec_contract: near_deposit")
    );
}

#[tokio::test]
#[serial(harvest)]
async fn test_start_boundary_values() {
    initialize();
    let required_balance = DEFAULT_REQUIRED_BALANCE.as_yoctonear();

    // Just below 128x
    let client = MockClient::new(0, required_balance * 127, required_balance * 127);
    let wallet = MockWallet::new();

    let result = start(&client, &wallet, &WNEAR_TOKEN, None).await;
    assert!(result.is_ok());
    assert!(!client.operations_log.contains("transfer_native_token"));

    // Above 128x with harvest time condition met
    let client = MockClient::new(
        required_balance * 256, // Set native balance high enough for harvest
        required_balance * 129, // Set wrapped balance above 128x
        required_balance * 129,
    );
    let wallet = MockWallet::new();

    // Set last harvest time to 24 hours ago
    LAST_HARVEST.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - harvest_interval()
            - 1,
        Ordering::Relaxed,
    );

    let result = start(&client, &wallet, &WNEAR_TOKEN, None).await;
    assert!(result.is_ok());

    assert!(client.operations_log.contains("transfer_native_token"));
}

#[tokio::test]
#[serial(harvest)]
async fn test_start_exact_upper() {
    initialize();
    let required_balance = DEFAULT_REQUIRED_BALANCE.as_yoctonear();

    // Exactly 128x
    let client = MockClient::new(
        required_balance << 7, // native balance
        required_balance << 7, // wrapped balance
        required_balance << 7,
    );
    let wallet = MockWallet::new();

    // Set last harvest time to 24 hours ago
    LAST_HARVEST.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - harvest_interval()
            - 1,
        Ordering::Relaxed,
    );

    let result = start(&client, &wallet, &WNEAR_TOKEN, None).await;
    assert!(result.is_ok());

    // Wait a bit to ensure any async operations complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Should not trigger harvest when exactly at upper limit
    assert!(!client.operations_log.contains("transfer_native_token"));
}

#[tokio::test]
#[serial(harvest)]
async fn test_start_harvest_time_condition() {
    initialize();
    let required_balance = DEFAULT_REQUIRED_BALANCE.as_yoctonear();

    // Set balance above 128x to meet the balance condition
    let client = MockClient::new(
        required_balance << 8, // 256x native balance
        required_balance << 8, // 256x wrapped balance
        required_balance << 8,
    );
    let wallet = MockWallet::new();

    // Set last harvest time to 12 hours ago (less than harvest_interval())
    LAST_HARVEST.store(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            - harvest_interval() / 2, // 12 hours ago
        Ordering::Relaxed,
    );

    let result = start(&client, &wallet, &WNEAR_TOKEN, None).await;
    assert!(result.is_ok());

    // Wait a bit to ensure any async operations complete
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Should not trigger harvest when time condition is not met
    assert!(!client.operations_log.contains("transfer_native_token"));

    // Verify that get_deposits was called (normal operation)
    assert!(
        client
            .operations_log
            .contains("view_contract: get_deposits")
    );
}
