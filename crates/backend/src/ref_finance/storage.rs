use crate::Result;
use crate::jsonrpc::{SendTx, SentTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::{CONTRACT_ADDRESS, deposit};
use crate::wallet::Wallet;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageBalanceBounds {
    pub min: U128,
    pub max: Option<U128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct StorageBalance {
    pub total: U128,
    pub available: U128,
}

pub async fn check_bounds<C: ViewContract>(client: &C) -> Result<StorageBalanceBounds> {
    let log = DEFAULT.new(o!("function" => "storage::check_bounds"));
    const METHOD_NAME: &str = "storage_balance_bounds";
    let args = json!({});
    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let bounds: StorageBalanceBounds = serde_json::from_slice(&result.result)?;
    trace!(log, "bounds"; "min" => ?bounds.min, "max" => ?bounds.max);
    Ok(bounds)
}

pub async fn deposit<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    value: NearToken,
    registration_only: bool,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!("function" => "storage::deposit"));
    const METHOD_NAME: &str = "storage_deposit";
    let args = json!({
        "registration_only": registration_only,
    });
    let signer = wallet.signer();
    info!(log, "depositing";
        "value" => value.as_yoctonear(),
        "signer" => ?signer.account_id,
    );

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, value)
        .await
}

pub async fn balance_of<C: ViewContract>(
    client: &C,
    account: &AccountId,
) -> Result<Option<StorageBalance>> {
    let log = DEFAULT.new(o!("function" => "storage::balance_of"));
    const METHOD_NAME: &str = "storage_balance_of";
    let args = json!({
        "account_id": account,
    });
    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let balance: Option<StorageBalance> = serde_json::from_slice(&result.result)?;
    if let Some(b) = &balance {
        trace!(log, "balance";
            "total" => ?b.total,
            "available" => ?b.available,
        );
    } else {
        trace!(log, "no balance");
    }
    Ok(balance)
}

// 現状の deposits を確認し、削除すべき token と追加すべき deposit を返す
pub async fn check_deposits<C: ViewContract>(
    client: &C,
    account: &AccountId,
    tokens: &[TokenAccount],
) -> Result<Option<(Vec<TokenAccount>, u128)>> {
    let log = DEFAULT.new(o!("function" => "storage::check_deposits"));

    let bounds = check_bounds(client).await?;
    let deposits = deposit::get_deposits(client, account).await?;
    if deposits.is_empty() {
        return Ok(None);
    }
    let maybe_balance = balance_of(client, account).await?;
    if maybe_balance.is_none() {
        return Ok(None);
    }
    let balance = maybe_balance.unwrap();

    let total = balance.total.0;
    let available = balance.available.0;
    let used = total - available;
    let per_token = (used - bounds.min.0) / deposits.len() as u128;

    trace!(log, "checking deposits";
        "total" => total,
        "available" => available,
        "used" => used,
        "per_token" => per_token,
    );

    let mores: Vec<_> = tokens
        .iter()
        .filter(|&token| !deposits.contains_key(token))
        .collect();
    let more_needed = mores.len() as u128 * per_token;
    debug!(log, "missing token deposits"; "more_needed" => more_needed);
    if more_needed <= available {
        return Ok(Some((vec![], 0)));
    }

    let shortage = more_needed - available;
    let mut needing_count = (shortage / per_token) as usize;
    if !shortage.is_multiple_of(per_token) {
        needing_count += 1;
    }
    let mut noneeds: Vec<_> = deposits
        .into_iter()
        .filter(|(token, amount)| !tokens.contains(token) && amount.0.is_zero())
        .map(|(token, _)| token)
        .collect();

    if needing_count < noneeds.len() {
        noneeds.drain(needing_count..);
    }
    if needing_count <= noneeds.len() {
        return Ok(Some((noneeds, 0)));
    }

    let more_posts = needing_count - noneeds.len();
    let more = more_posts as u128 * per_token;

    Ok(Some((noneeds, more)))
}

pub async fn check_and_deposit<C, W>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<Option<()>>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "storage::check_and_deposit"));
    let account = wallet.account_id();
    let maybe_res = check_deposits(client, account, tokens).await?;
    if maybe_res.is_none() {
        return Ok(None);
    }
    let (deleting_tokens, more) = maybe_res.unwrap();
    if !deleting_tokens.is_empty() {
        deposit::unregister_tokens(client, wallet, &deleting_tokens)
            .await?
            .wait_for_success()
            .await?;
    }
    if more > 0 {
        info!(log, "needing more deposit"; "more" => more);
        deposit(client, wallet, NearToken::from_yoctonear(more), false)
            .await?
            .wait_for_success()
            .await?;
    }
    Ok(Some(()))
}

/// REF Finance のストレージセットアップを確認し、必要に応じて初期化を実行する
///
/// この関数は以下を実行します:
/// 1. storage_balance_of でアカウントの登録状態を確認
/// 2. 未登録の場合は storage_deposit を実行
/// 3. 指定されたトークンを register_tokens で一括登録
///
/// # Arguments
/// * `client` - NEAR RPCクライアント
/// * `wallet` - ウォレット
/// * `tokens` - 登録するトークンのリスト
pub async fn ensure_ref_storage_setup<C, W>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<()>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "storage::ensure_ref_storage_setup"));
    let account = wallet.account_id();

    trace!(log, "checking REF Finance storage setup"; "account" => %account, "token_count" => tokens.len());

    // 1. storage_balance_of でアカウント状態を確認
    let maybe_balance = balance_of(client, account).await?;

    // 2. 未登録または不足している場合は storage_deposit を実行
    if maybe_balance.is_none() {
        trace!(
            log,
            "account not registered, performing initial storage deposit"
        );
        let bounds = check_bounds(client).await?;
        let min_deposit = bounds.min.0;

        deposit(
            client,
            wallet,
            NearToken::from_yoctonear(min_deposit),
            false,
        )
        .await?
        .wait_for_success()
        .await?;

        trace!(log, "initial storage deposit completed"; "amount" => min_deposit);
    } else {
        trace!(log, "account already registered");
    }

    // 3. トークンを一括登録（未登録のもののみ）
    if !tokens.is_empty() {
        // 既に登録済みのトークンを取得
        let registered_tokens = deposit::get_deposits(client, account).await?;

        // 未登録のトークンのみをフィルタリング
        let unregistered_tokens: Vec<TokenAccount> = tokens
            .iter()
            .filter(|token| !registered_tokens.contains_key(token))
            .cloned()
            .collect();

        if !unregistered_tokens.is_empty() {
            trace!(log, "registering unregistered tokens";
                "total" => tokens.len(),
                "already_registered" => tokens.len() - unregistered_tokens.len(),
                "to_register" => unregistered_tokens.len()
            );
            deposit::register_tokens(client, wallet, &unregistered_tokens)
                .await?
                .wait_for_success()
                .await?;
            trace!(log, "tokens registered successfully"; "count" => unregistered_tokens.len());
        } else {
            trace!(log, "all tokens already registered"; "count" => tokens.len());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
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
}
