use super::*;
use crate::jsonrpc::SentTx;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::NearToken;
use std::cell::Cell;
use std::collections::HashMap;

struct MockStorage(StorageBalanceBounds);

impl ViewContract for MockStorage {
    async fn view_contract<T>(&self, _: &AccountId, _: &str, _: &T) -> Result<CallResult>
    where
        T: ?Sized + serde::Serialize,
    {
        Ok(CallResult {
            result: serde_json::to_vec(&self.0)?,
            logs: vec![],
        })
    }
}

#[tokio::test]
async fn test_check_bounds() {
    let sbb = StorageBalanceBounds {
        min: U128(1_000_000_000_000_000_000_000),
        max: Some(U128(0)),
    };
    let client = MockStorage(sbb);
    let res = check_bounds(&client).await;
    assert!(res.is_ok());
    let bounds = res.unwrap();
    assert!(bounds.min >= U128(1_000_000_000_000_000_000_000));
    assert!(bounds.max.unwrap_or(U128(0)) >= U128(0));
}

// MockWallet for storage tests
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

// MockSentTx for storage tests
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

// Comprehensive MockClient for storage tests
struct MockStorageClient {
    storage_balance: std::cell::RefCell<Option<StorageBalance>>,
    storage_bounds: StorageBalanceBounds,
    deposits: HashMap<TokenAccount, U128>,
    should_fail_deposit: Cell<bool>,
}

unsafe impl Sync for MockStorageClient {}

impl MockStorageClient {
    fn new_with_balance(balance: StorageBalance) -> Self {
        Self {
            storage_balance: std::cell::RefCell::new(Some(balance)),
            storage_bounds: StorageBalanceBounds {
                min: U128(1_000_000_000_000_000_000_000), // 0.001 NEAR
                max: None,
            },
            deposits: HashMap::new(),
            should_fail_deposit: Cell::new(false),
        }
    }

    fn new_unregistered() -> Self {
        Self {
            storage_balance: std::cell::RefCell::new(None),
            storage_bounds: StorageBalanceBounds {
                min: U128(1_000_000_000_000_000_000_000),
                max: None,
            },
            deposits: HashMap::new(),
            should_fail_deposit: Cell::new(false),
        }
    }

    fn with_deposits(mut self, deposits: HashMap<TokenAccount, U128>) -> Self {
        self.deposits = deposits;
        self
    }
}

impl ViewContract for MockStorageClient {
    async fn view_contract<T>(
        &self,
        _receiver: &AccountId,
        method_name: &str,
        _args: &T,
    ) -> Result<CallResult>
    where
        T: ?Sized + serde::Serialize,
    {
        let result = match method_name {
            "storage_balance_of" => serde_json::to_vec(&*self.storage_balance.borrow())?,
            "storage_balance_bounds" => serde_json::to_vec(&self.storage_bounds)?,
            "get_deposits" => serde_json::to_vec(&self.deposits)?,
            _ => serde_json::to_vec(&serde_json::Value::Null)?,
        };
        Ok(CallResult {
            result,
            logs: vec![],
        })
    }
}

impl crate::jsonrpc::SendTx for MockStorageClient {
    type Output = MockSentTx;

    async fn transfer_native_token(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _amount: NearToken,
    ) -> Result<Self::Output> {
        Ok(MockSentTx { should_fail: false })
    }

    async fn exec_contract<T>(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        method_name: &str,
        _args: T,
        _deposit: NearToken,
    ) -> Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        let should_fail = self.should_fail_deposit.get();
        if method_name == "storage_deposit" && !should_fail {
            // Simulate successful storage deposit
            *self.storage_balance.borrow_mut() = Some(StorageBalance {
                total: self.storage_bounds.min,
                available: U128(0),
            });
        }
        Ok(MockSentTx { should_fail })
    }

    async fn send_tx(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<Action>,
    ) -> Result<Self::Output> {
        Ok(MockSentTx { should_fail: false })
    }
}

// Test: balance_of with balance
#[tokio::test]
async fn test_balance_of_with_balance() {
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(500_000_000_000_000_000_000),
    };
    let client = MockStorageClient::new_with_balance(balance.clone());
    let account: AccountId = "test.near".parse().unwrap();

    let result = balance_of(&client, &account).await;
    assert!(result.is_ok());
    let returned_balance = result.unwrap();
    assert!(returned_balance.is_some());
    let b = returned_balance.unwrap();
    assert_eq!(b.total, balance.total);
    assert_eq!(b.available, balance.available);
}

// Test: balance_of no account
#[tokio::test]
async fn test_balance_of_no_account() {
    let client = MockStorageClient::new_unregistered();
    let account: AccountId = "unknown.near".parse().unwrap();

    let result = balance_of(&client, &account).await;
    assert!(result.is_ok());
    let returned_balance = result.unwrap();
    assert!(returned_balance.is_none());
}

// Test: storage_deposit
#[tokio::test]
async fn test_storage_deposit() {
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();
    let deposit_amount = NearToken::from_yoctonear(1_000_000_000_000_000_000_000);

    let result = deposit(&client, &wallet, deposit_amount, false).await;
    assert!(result.is_ok());
}

// Test: check_deposits sufficient
#[tokio::test]
async fn test_check_deposits_sufficient() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(1_000_000));

    // Balance with enough available
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(1_000_000_000_000_000_000_000), // Sufficient available
    };
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let account: AccountId = "test.near".parse().unwrap();
    let tokens = vec![token];

    let result = check_deposits(&client, &account, &tokens).await;
    assert!(result.is_ok());
    let maybe_result = result.unwrap();
    assert!(maybe_result.is_some());
    let (to_delete, more_needed) = maybe_result.unwrap();
    assert!(to_delete.is_empty());
    assert_eq!(more_needed, 0);
}

// Test: check_deposits need more
#[tokio::test]
async fn test_check_deposits_need_more() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let new_token: TokenAccount = "usdt.near".parse().unwrap();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(1_000_000));

    // Balance with limited available (needs more for new token)
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0), // No available, needs more
    };
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let account: AccountId = "test.near".parse().unwrap();
    let tokens = vec![token, new_token];

    let result = check_deposits(&client, &account, &tokens).await;
    assert!(result.is_ok());
    let maybe_result = result.unwrap();
    assert!(maybe_result.is_some());
    let (_to_delete, more_needed) = maybe_result.unwrap();
    // Should need more since available is 0 and we have a new token
    assert!(more_needed > 0);
}

// Test: check_and_deposit
#[tokio::test]
async fn test_check_and_deposit() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(1_000_000));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(1_000_000_000_000_000_000_000),
    };
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let result = check_and_deposit(&client, &wallet, &tokens).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - already registered
#[tokio::test]
async fn test_ensure_ref_storage_setup_already_registered() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(0));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(500_000_000_000_000_000_000),
    };
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let result = ensure_ref_storage_setup(&client, &wallet, &tokens).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - unregistered account
#[tokio::test]
async fn test_ensure_ref_storage_setup_unregistered() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let result = ensure_ref_storage_setup(&client, &wallet, &tokens).await;
    assert!(result.is_ok());
}
