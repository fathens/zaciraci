use super::*;
use crate::jsonrpc::SentTx;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::NearToken;
use std::cell::Cell;
use std::sync::{Arc, Mutex};

struct MockClient(HashMap<TokenAccount, U128>);

impl ViewContract for MockClient {
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
async fn test_get_deposits() {
    let token: TokenAccount = "wrap.testnet".parse().unwrap();
    let account = "app.zaciraci.testnet".parse().unwrap();
    let map = vec![(token.clone(), U128(100))].into_iter().collect();
    let client = MockClient(map);
    let result = get_deposits(&client, &account).await;
    assert!(result.is_ok());
    let deposits = result.unwrap();
    assert!(!deposits.is_empty());
    assert!(deposits.contains_key(&token));
}

// MockWallet for deposit tests
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

impl crate::wallet::Wallet for MockWallet {
    fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    fn signer(&self) -> &InMemorySigner {
        &self.signer
    }
}

// MockSentTx for deposit tests
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

// Comprehensive MockClient for deposit tests
struct MockDepositClient {
    wnear_balance: Cell<u128>,
    deposits: HashMap<TokenAccount, U128>,
    last_method: Arc<Mutex<Option<String>>>,
    should_fail: Cell<bool>,
}

unsafe impl Sync for MockDepositClient {}

impl MockDepositClient {
    fn new(wnear_balance: u128) -> Self {
        Self {
            wnear_balance: Cell::new(wnear_balance),
            deposits: HashMap::new(),
            last_method: Arc::new(Mutex::new(None)),
            should_fail: Cell::new(false),
        }
    }

    fn with_deposits(mut self, deposits: HashMap<TokenAccount, U128>) -> Self {
        self.deposits = deposits;
        self
    }
}

impl ViewContract for MockDepositClient {
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
            "ft_balance_of" => serde_json::to_vec(&U128(self.wnear_balance.get()))?,
            "get_deposits" => serde_json::to_vec(&self.deposits)?,
            _ => serde_json::to_vec(&serde_json::Value::Null)?,
        };
        Ok(CallResult {
            result,
            logs: vec![],
        })
    }
}

impl crate::jsonrpc::SendTx for MockDepositClient {
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
        *self.last_method.lock().unwrap() = Some(method_name.to_string());
        let should_fail = self.should_fail.get();
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

// Test: wnear::balance_of
#[tokio::test]
async fn test_wnear_balance_of() {
    let expected_balance = 1_000_000_000_000_000_000_000_000u128; // 1 wNEAR
    let client = MockDepositClient::new(expected_balance);
    let account: AccountId = "test.near".parse().unwrap();

    let result = wnear::balance_of(&client, &account).await;
    assert!(result.is_ok());
    let balance = result.unwrap();
    assert_eq!(balance.as_yoctonear(), expected_balance);
}

// Test: wnear::wrap
#[tokio::test]
async fn test_wnear_wrap() {
    let client = MockDepositClient::new(0);
    let wallet = MockWallet::new();
    let amount = NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000);

    let result = wnear::wrap(&client, &wallet, amount).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("near_deposit".to_string()));
}

// Test: wnear::unwrap
#[tokio::test]
async fn test_wnear_unwrap() {
    let client = MockDepositClient::new(1_000_000_000_000_000_000_000_000);
    let wallet = MockWallet::new();
    let amount = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);

    let result = wnear::unwrap(&client, &wallet, amount).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("near_withdraw".to_string()));
}

// Test: deposit (ft_transfer_call)
#[tokio::test]
async fn test_deposit_ft_transfer_call() {
    let client = MockDepositClient::new(0);
    let wallet = MockWallet::new();
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let amount = NearToken::from_yoctonear(1_000_000_000_000_000_000_000_000);

    let result = deposit(&client, &wallet, &token, amount).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("ft_transfer_call".to_string()));
}

// Test: withdraw
#[tokio::test]
async fn test_withdraw() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(1_000_000_000_000_000_000_000_000));

    let client = MockDepositClient::new(0).with_deposits(deposits);
    let wallet = MockWallet::new();
    let amount = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);

    let result = withdraw(&client, &wallet, &token, amount).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("withdraw".to_string()));
}

// Test: register_tokens
#[tokio::test]
async fn test_register_tokens() {
    let client = MockDepositClient::new(0);
    let wallet = MockWallet::new();
    let tokens: Vec<TokenAccount> = vec![WNEAR_TOKEN.clone(), "usdt.near".parse().unwrap()];

    let result = register_tokens(&client, &wallet, &tokens).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("register_tokens".to_string()));
}

// Test: unregister_tokens
#[tokio::test]
async fn test_unregister_tokens() {
    let token: TokenAccount = "usdt.near".parse().unwrap();
    let mut deposits = HashMap::new();
    deposits.insert(token.clone(), U128(0)); // 残高ゼロで登録解除可能

    let client = MockDepositClient::new(0).with_deposits(deposits);
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let result = unregister_tokens(&client, &wallet, &tokens).await;
    assert!(result.is_ok());

    let last_method = client.last_method.lock().unwrap().clone();
    assert_eq!(last_method, Some("unregister_tokens".to_string()));
}
