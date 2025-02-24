#![allow(dead_code)]

use crate::config;
use crate::jsonrpc::{AccountInfo, SendTx, SentTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use crate::wallet::Wallet;
use crate::Result;
use futures_util::FutureExt;
use near_primitives::types::Balance;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Once;

const DEFAULT_REQUIRED_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const MINIMUM_NATIVE_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const INTERVAL_OF_HARVEST: u64 = 24 * 60 * 60;

static LAST_HARVEST: AtomicU64 = AtomicU64::new(0);
static HARVEST_ACCOUNT: Lazy<AccountId> = Lazy::new(|| {
    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|err| panic!("{}", err));
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse config `{}`: {}", value, err))
});

static INIT: Once = Once::new();

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > INTERVAL_OF_HARVEST
}

fn update_last_harvest() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST.store(now, Ordering::Relaxed);
}

pub async fn start<C, W>(client: &C, wallet: &W) -> Result<(TokenAccount, Balance)>
where
    C: AccountInfo + SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_balance = {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_BALANCE
        } else {
            max
        }
    };
    info!(log, "Starting balances";
        "required_balance" => %required_balance,
    );

    let token = WNEAR_TOKEN.clone();

    let wrapped_balance = balance_of_start_token(client, wallet, &token).await?;
    info!(log, "comparing";
        "wrapped_balance" => wrapped_balance,
    );

    if wrapped_balance < required_balance {
        refill(client, wallet, required_balance - wrapped_balance).await?;
        Ok((token, wrapped_balance))
    } else {
        let upper = required_balance << 4;
        if upper < wrapped_balance {
            let _ignore_response =
                harvest(client, wallet, &WNEAR_TOKEN, wrapped_balance - upper, upper).map(|r| {
                    match r {
                        Ok(_) => info!(log, "successfully harvested"),
                        Err(err) => warn!(log, "failed to harvest: {}", err),
                    }
                });
        }
        Ok((token, upper))
    }
}

async fn balance_of_start_token<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
) -> Result<Balance>
where
    C: AccountInfo + ViewContract,
    W: Wallet,
{
    let account = wallet.account_id();
    let deposits = deposit::get_deposits(client, account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill<C, W>(client: &C, wallet: &W, want: Balance) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => format!("{}", want),
    ));
    let account = wallet.account_id();
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;
    let log = log.new(o!(
        "wrapped_balance" => format!("{}", wrapped_balance),
    ));
    debug!(log, "checking");

    let actual_wrapping = if wrapped_balance < want {
        let wrapping = want - wrapped_balance;
        let native_balance = client.get_native_amount(account).await?;
        let log = log.new(o!(
            "native_balance" => format!("{}", native_balance),
            "wrapping" => format!("{}", wrapping),
        ));
        debug!(log, "checking");
        let available = native_balance
            .checked_sub(MINIMUM_NATIVE_BALANCE)
            .unwrap_or_default();

        let amount = if available < wrapping {
            info!(log, "insufficient balance, using maximum available";
                "available" => %available,
                "wanted" => %wrapping,
            );
            available
        } else {
            wrapping
        };

        if amount > 0 {
            info!(log, "wrapping";
                "amount" => %amount,
            );
            deposit::wnear::wrap(client, wallet, amount)
                .await?
                .wait_for_success()
                .await?;
        }
        amount
    } else {
        0
    };

    let total_deposit = wrapped_balance + actual_wrapping;
    if total_deposit > 0 {
        info!(log, "refilling";
            "amount" => %total_deposit,
        );
        deposit::deposit(client, wallet, &WNEAR_TOKEN, total_deposit)
            .await?
            .wait_for_success()
            .await?;
    } else {
        info!(log, "no amount to deposit")
    }
    Ok(())
}

pub async fn harvest<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    withdraw: Balance,
    required: Balance,
) -> Result<()>
where
    C: AccountInfo + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.harvest",
        "withdraw" => format!("{}", withdraw),
        "required" => format!("{}", required),
    ));
    info!(log, "withdrawing";
        "token" => %token,
    );
    deposit::withdraw(client, wallet, token, withdraw)
        .await?
        .wait_for_success()
        .await?;
    let account = wallet.account_id();
    let native_balance = client.get_native_amount(account).await?;
    let upper = required << 4;
    info!(log, "checking";
        "native_balance" => %native_balance,
        "upper" => %upper,
    );
    if upper < native_balance && is_time_to_harvest() {
        let amount = native_balance - upper;
        let target = &*HARVEST_ACCOUNT;
        info!(log, "harvesting";
            "target" => %target,
            "amount" => %amount,
        );
        let signer = wallet.signer();
        client
            .transfer_native_token(signer, target, amount)
            .await?
            .wait_for_success()
            .await?;
        update_last_harvest()
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use near_crypto::InMemorySigner;
    use near_primitives::transaction::Action;
    use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
    use near_sdk::json_types::U128;
    use serde_json::json;
    use std::cell::{Cell, RefCell};
    use std::sync::Once;

    static INIT: Once = Once::new();

    struct MockWallet {
        account_id: AccountId,
        signer: InMemorySigner,
    }

    impl MockWallet {
        fn new() -> Self {
            let account_id: AccountId = "test.near".parse().unwrap();
            let signer = InMemorySigner::from_seed(
                account_id.clone(),
                near_crypto::KeyType::ED25519,
                "test.near",
            );
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
            std::env::set_var("HARVEST_ACCOUNT_ID", "harvest.near");
        });
    }

    struct OperationsLog(RefCell<Vec<String>>);

    impl OperationsLog {
        fn new() -> Self {
            Self(RefCell::new(Vec::new()))
        }

        fn push(&self, op: String) {
            self.0.borrow_mut().push(op);
        }

        fn contains(&self, s: &str) -> bool {
            self.0.borrow().iter().any(|log| log.contains(s))
        }
    }

    struct MockClient {
        native_amount: Cell<Balance>,
        wnear_amount: Cell<Balance>,
        wnear_deposited: Cell<Balance>,
        operations_log: OperationsLog,
        should_fail_near_deposit: Cell<bool>,
        should_fail_ft_transfer: Cell<bool>,
    }

    impl MockClient {
        fn new(native: Balance, wnear: Balance, deposited: Balance) -> Self {
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
        async fn get_native_amount(&self, _account: &AccountId) -> Result<Balance> {
            self.log_operation("get_native_amount");
            Ok(self.native_amount.get())
        }
    }

    impl SendTx for MockClient {
        type Output = MockSentTx;

        async fn transfer_native_token(
            &self,
            _signer: &InMemorySigner,
            _receiver: &AccountId,
            _amount: Balance,
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
            deposit: Balance,
        ) -> Result<Self::Output>
        where
            T: Sized + serde::Serialize,
        {
            self.log_operation(&format!("exec_contract: {method_name}"));
            let should_fail = match method_name {
                "near_deposit" => {
                    if !self.should_fail_near_deposit.get() {
                        self.native_amount
                            .set(self.native_amount.get().saturating_sub(deposit));
                        self.wnear_amount
                            .set(self.wnear_amount.get().saturating_add(deposit));
                    }
                    self.should_fail_near_deposit.get()
                }
                "ft_transfer_call" => {
                    if !self.should_fail_ft_transfer.get() {
                        let args_str = serde_json::to_string(&args).unwrap();
                        let args_value: serde_json::Value =
                            serde_json::from_str(&args_str).unwrap();
                        let amount: Balance = args_value["amount"]
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
            T: ?Sized + serde::Serialize,
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
                gas_burnt: 0,
                tokens_burnt: 0,
                executor_id: AccountId::try_from("test.near".to_string()).unwrap(),
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

        let client = MockClient::new(DEFAULT_REQUIRED_BALANCE << 5, 0, 0);
        let wallet = MockWallet::new();

        let result = start(&client, &wallet).await;
        let (token, balance) = result.unwrap();
        assert_eq!(token, WNEAR_TOKEN.clone());
        assert!(balance.is_zero());

        assert!(client
            .operations_log
            .contains("view_contract: get_deposits"));
    }

    #[tokio::test]
    async fn test_harvest_with_sufficient_balance() {
        initialize();

        let required = 1_000_000;
        let native_balance = required << 5;
        let client = MockClient::new(native_balance, 0, 0);
        let wallet = MockWallet::new();

        LAST_HARVEST.store(0, Ordering::Relaxed);

        let result = harvest(&client, &wallet, &WNEAR_TOKEN, 1000, required).await;
        assert!(result.is_ok());

        assert!(client.operations_log.contains("get_native_amount"));
    }

    #[tokio::test]
    async fn test_harvest_with_insufficient_balance() {
        initialize();

        let required = 1_000_000;
        let native_balance = required << 3;
        let client = MockClient::new(native_balance, 0, 0);
        let wallet = MockWallet::new();

        let result = harvest(&client, &wallet, &WNEAR_TOKEN, 1000, required).await;
        assert!(result.is_ok());

        assert!(client.operations_log.contains("get_native_amount"));
    }

    #[test]
    fn test_is_time_to_harvest() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST - 1, Ordering::Relaxed);
        assert!(is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 1, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 2, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
    }

    #[test]
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

        let required = 1_000_000;
        let client = MockClient::new(required << 1, required << 1, required << 1);
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());

        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
    }

    #[tokio::test]
    async fn test_refill_with_sufficient_native_balance() {
        initialize();

        let required = NearToken::from_near(2).as_yoctonear();
        let client = MockClient::new(required << 2, 0, 0);
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());

        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
    }

    #[tokio::test]
    async fn test_refill_with_insufficient_native_balance() {
        initialize();

        let required = 1_000_000;
        let client = MockClient::new(MINIMUM_NATIVE_BALANCE, 0, 0);
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());

        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));
    }

    #[test]
    fn test_u128_serialization() {
        let amount = U128(1_000_000);
        let args = json!({
            "receiver_id": "contract.near",
            "amount": amount,
            "msg": "",
        });
        println!("args: {:?}", args);
        let args_str = serde_json::to_string(&args).unwrap();
        println!("args_str: {}", args_str);
        let args_value: serde_json::Value = serde_json::from_str(&args_str).unwrap();
        println!("args_value: {:?}", args_value);
    }

    #[tokio::test]
    async fn test_refill_scenarios() {
        initialize();
        let required = NearToken::from_near(2).as_yoctonear();

        // Case 1: Wrapped残高が十分ある
        let client = MockClient::new(required + MINIMUM_NATIVE_BALANCE, 0, required);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));

        // Case 2: Wrapped残高が不足、Native残高が十分
        let client = MockClient::new(required + MINIMUM_NATIVE_BALANCE * 2, 0, 0);
        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
        assert!(client.operations_log.contains("exec_contract: ft_transfer_call"));
    }

    #[tokio::test]
    async fn test_refill_edge_cases() {
        initialize();

        // Case 1: want が 0 の場合
        let client = MockClient::new(1_000_000, 0, 0);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, 0).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));

        // Case 2: 非常に大きな want 値の場合
        let large_want = u128::MAX / 2;
        let client = MockClient::new(large_want, 0, 0);
        let result = refill(&client, &wallet, large_want).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));

        // Case 3: ネイティブ残高がちょうど MINIMUM_NATIVE_BALANCE の場合
        let required = 1_000_000;
        let client = MockClient::new(MINIMUM_NATIVE_BALANCE, 0, 0);
        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));
    }

    #[tokio::test]
    async fn test_refill_boundary_conditions() {
        initialize();

        // Case 1: want値がMINIMUM_NATIVE_BALANCEと同じ
        let client = MockClient::new(MINIMUM_NATIVE_BALANCE * 3, 0, 0);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, MINIMUM_NATIVE_BALANCE).await;
        assert!(result.is_ok());

        // Case 2: Native残高がwant + MINIMUM_NATIVE_BALANCEちょうど
        let want = 1_000_000;
        let client = MockClient::new(want + MINIMUM_NATIVE_BALANCE, 0, 0);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_refill_overflow_conditions() {
        initialize();

        // Case 1: want値が非常に大きく、Native残高との加算でオーバーフローする可能性
        let want = u128::MAX - MINIMUM_NATIVE_BALANCE + 1;
        let client = MockClient::new(u128::MAX, 0, 0);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());

        // Case 2: Native残高とWrapped残高の合計が要求額に満たない
        let want = 1_000_000;
        let native = want / 2;
        let wrapped = want / 4;
        let client = MockClient::new(native, wrapped, wrapped);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_refill_transaction_failures() {
        initialize();

        // Case 1: near_depositトランザクションが失敗する場合
        let want = 1_000_000;
        let native = want * 2 + MINIMUM_NATIVE_BALANCE;
        let wrapped = 0;
        let client = MockClient::new(native, wrapped, 0);
        let wallet = MockWallet::new();
        client.set_near_deposit_failure(true);

        let result = refill(&client, &wallet, want).await;
        assert!(result.is_err());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
        assert!(result.unwrap_err().to_string().contains("Transaction failed"));

        // Case 2: ft_transfer_callトランザクションが失敗する場合
        let client = MockClient::new(native, wrapped, 0);
        client.set_ft_transfer_failure(true);

        let result = refill(&client, &wallet, want * 2).await;
        assert!(result.is_err());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
        assert!(client.operations_log.contains("exec_contract: ft_transfer_call"));
        assert!(result.unwrap_err().to_string().contains("Transaction failed"));
    }

    #[tokio::test]
    async fn test_refill_combined_balances() {
        initialize();

        // Case 1: wrapped残高とnative残高の合計が要求額を満たす
        let want = 1_000_000;
        let wrapped = want / 2;
        let native = want / 2 + MINIMUM_NATIVE_BALANCE;
        let client = MockClient::new(native, wrapped, wrapped);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("view_contract: ft_balance_of"));
        assert!(client.operations_log.contains("get_native_amount"));

        // Case 2: wrapped残高が一部あり、native残高から補充
        let want = 1_000_000;
        let wrapped = want / 4;
        let native = want + MINIMUM_NATIVE_BALANCE;
        let client = MockClient::new(native, wrapped, wrapped);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
    }

    #[tokio::test]
    async fn test_refill_minimum_balances() {
        initialize();

        // Case 1: ちょうどMINIMUM_NATIVE_BALANCEのnative残高
        let want = 1_000;
        let client = MockClient::new(MINIMUM_NATIVE_BALANCE, 0, 0);
        let wallet = MockWallet::new();
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("get_native_amount"));

        // Case 2: MINIMUM_NATIVE_BALANCEより少し多いnative残高
        let client = MockClient::new(MINIMUM_NATIVE_BALANCE + 1, 0, 0);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        assert!(client.operations_log.contains("exec_contract: near_deposit"));
    }

    #[tokio::test]
    async fn test_refill_transaction_order() {
        initialize();

        // トランザクションの順序を確認
        let want = 1_000_000;
        let native = want * 2 + MINIMUM_NATIVE_BALANCE;
        let client = MockClient::new(native, 0, 0);
        let wallet = MockWallet::new();
        
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        
        let logs = client.operations_log.0.borrow();
        let near_deposit_idx = logs.iter().position(|log| log.contains("near_deposit")).unwrap();
        let ft_transfer_idx = logs.iter().position(|log| log.contains("ft_transfer_call")).unwrap();
        assert!(near_deposit_idx < ft_transfer_idx, "near_deposit should be called before ft_transfer_call");
    }

    #[tokio::test]
    async fn test_refill_error_recovery() {
        initialize();

        let want = 1_000_000;
        let native = want * 2 + MINIMUM_NATIVE_BALANCE;
        let client = MockClient::new(native, 0, 0);
        let wallet = MockWallet::new();

        // Case 1: 最初のnear_depositが失敗し、2回目で成功
        client.set_near_deposit_failure(true);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_err());
        
        client.set_near_deposit_failure(false);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
        
        // Case 2: ft_transfer_callが失敗し、再試行で成功
        client.set_ft_transfer_failure(true);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_err());
        
        client.set_ft_transfer_failure(false);
        let result = refill(&client, &wallet, want).await;
        assert!(result.is_ok());
    }
}
