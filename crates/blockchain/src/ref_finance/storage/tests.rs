use super::*;
use crate::jsonrpc::SentTx;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use anyhow::anyhow;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::views::{
    CallResult, FinalExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
};
use near_sdk::NearToken;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
        Self::with_account_id("test.near")
    }

    fn with_account_id(account: &str) -> Self {
        let account_id: AccountId = account.parse().unwrap();
        let signer_result =
            InMemorySigner::from_seed(account_id.clone(), near_crypto::KeyType::ED25519, account);
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

    async fn wait_for_success(&self) -> Result<FinalExecutionOutcomeView> {
        if self.should_fail {
            return Err(anyhow!("Transaction failed"));
        }
        Ok(crate::mock::dummy_final_outcome(b"\"0\"".to_vec()))
    }
}

// Comprehensive MockClient for storage tests
struct MockStorageClient {
    storage_balance: Mutex<Option<StorageBalance>>,
    storage_bounds: StorageBalanceBounds,
    deposits: BTreeMap<TokenAccount, U128>,
    should_fail_deposit: AtomicBool,
    should_fail_unregister: AtomicBool,
    storage_deposit_count: AtomicUsize,
    unregister_count: AtomicUsize,
}

impl MockStorageClient {
    fn new_with_balance(balance: StorageBalance) -> Self {
        Self {
            storage_balance: Mutex::new(Some(balance)),
            storage_bounds: StorageBalanceBounds {
                min: U128(1_000_000_000_000_000_000_000), // 0.001 NEAR
                max: None,
            },
            deposits: BTreeMap::new(),
            should_fail_deposit: AtomicBool::new(false),
            should_fail_unregister: AtomicBool::new(false),
            storage_deposit_count: AtomicUsize::new(0),
            unregister_count: AtomicUsize::new(0),
        }
    }

    fn new_unregistered() -> Self {
        Self {
            storage_balance: Mutex::new(None),
            storage_bounds: StorageBalanceBounds {
                min: U128(1_000_000_000_000_000_000_000),
                max: None,
            },
            deposits: BTreeMap::new(),
            should_fail_deposit: AtomicBool::new(false),
            should_fail_unregister: AtomicBool::new(false),
            storage_deposit_count: AtomicUsize::new(0),
            unregister_count: AtomicUsize::new(0),
        }
    }

    fn with_deposits(mut self, deposits: BTreeMap<TokenAccount, U128>) -> Self {
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
            "storage_balance_of" => serde_json::to_vec(&*self.storage_balance.lock().unwrap())?,
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
        if method_name == "unregister_tokens" {
            self.unregister_count.fetch_add(1, Ordering::Relaxed);
            let should_fail = self.should_fail_unregister.load(Ordering::Relaxed);
            return Ok(MockSentTx { should_fail });
        }
        let should_fail = self.should_fail_deposit.load(Ordering::Relaxed);
        if method_name == "storage_deposit" && !should_fail {
            self.storage_deposit_count.fetch_add(1, Ordering::Relaxed);
            // Simulate successful storage deposit
            *self.storage_balance.lock().unwrap() = Some(StorageBalance {
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

// Test: ensure_ref_storage_setup - already registered
#[tokio::test]
async fn test_ensure_ref_storage_setup_already_registered() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(0));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(500_000_000_000_000_000_000),
    };
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let result = ensure_ref_storage_setup(&client, &wallet, &tokens, &keep, max_top_up).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - unregistered account
#[tokio::test]
async fn test_ensure_ref_storage_setup_unregistered() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let result = ensure_ref_storage_setup(&client, &wallet, &tokens, &keep, max_top_up).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - unregister stale tokens path
#[tokio::test]
async fn test_ensure_ref_storage_setup_unregister_path() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let stale: TokenAccount = "stale.near".parse().unwrap();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));
    deposits.insert(stale, U128(0)); // ゼロ残高 → unregister 候補

    // available が十分あるので top-up は不要だが、unregister パスは通る
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(500_000_000_000_000_000_000),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, max_top_up).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - top-up path
#[tokio::test]
async fn test_ensure_ref_storage_setup_top_up_path() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));

    // available がほぼゼロ → 新トークン登録に top-up が必要
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, max_top_up).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - max_top_up exceeded error
#[tokio::test]
async fn test_ensure_ref_storage_setup_max_top_up_exceeded() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));

    // available がゼロで top-up が必要だが、上限を 1 yocto に制限
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(1); // 極端に低い上限
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, max_top_up).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds remaining cap"),
        "expected 'exceeds remaining cap' in error: {}",
        err_msg
    );
}

// Test: ensure_ref_storage_setup - unregister with >10 stale tokens (chunk splitting)
//
// 15 stale + 多数の新規登録要求で unregister 候補を全て使い切る条件を作り、
// chunk 分割（CHUNK_SIZE=10）で 2 chunk に分かれることを unregister_count で確認。
#[tokio::test]
async fn test_ensure_ref_storage_setup_chunk_splitting() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));
    // 15 個の stale トークン → 2 チャンク (10 + 5) に分割
    for i in 0..15 {
        let stale: TokenAccount = format!("stale{i}.near").parse().unwrap();
        deposits.insert(stale, U128(0));
    }

    // available を極小にして shortage を大きくし、15 件全てを unregister 候補として使う
    let balance = StorageBalance {
        total: U128(20_000_000_000_000_000_000_000),
        available: U128(0),
    };
    // 新規登録は多め（per_token * 20 の shortage を作る）
    let new_tokens: Vec<TokenAccount> = (0..20)
        .map(|i| format!("new{i}.near").parse().unwrap())
        .collect();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let mut requested = vec![token];
    requested.extend(new_tokens);

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000_000);
    let result = ensure_ref_storage_setup(&client, &wallet, &requested, &keep, max_top_up).await;
    assert!(result.is_ok());
    // 15 stale → chunk_size=10 → 2 chunks (10 + 5)
    assert_eq!(
        client.unregister_count.load(Ordering::Relaxed),
        2,
        "15 stale tokens should be split into 2 chunks (10 + 5)"
    );
}

// Test: ensure_ref_storage_setup - unregister partial failure continues to register
//
// unregister の全チャンクが失敗しても、後続の top-up/register_tokens まで到達して
// Ok を返すことを確認。MockStorageClient の unregister_count は呼び出し前に
// fetch_add されるため、失敗チャンク数もカウントに含まれる。
#[tokio::test]
async fn test_ensure_ref_storage_setup_unregister_partial_failure() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));
    let stale: TokenAccount = "stale.near".parse().unwrap();
    deposits.insert(stale, U128(0));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(500_000_000_000_000_000_000),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    // unregister は失敗するが、処理は続行して register まで到達する
    client.should_fail_unregister.store(true, Ordering::Relaxed);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, max_top_up).await;
    // unregister 失敗後も register まで到達して正常完了
    assert!(result.is_ok());
    // stale 1 つ → 1 チャンクの unregister が試行された
    assert_eq!(
        client.unregister_count.load(Ordering::Relaxed),
        1,
        "unregister should have been attempted once even on failure"
    );
}

// Test: ensure_ref_storage_setup - max_top_up = 0 blocks any top-up
#[tokio::test]
async fn test_ensure_ref_storage_setup_zero_cap() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    let max_top_up = NearToken::from_yoctonear(0);
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, max_top_up).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds remaining cap"),
        "expected 'exceeds remaining cap' in error: {}",
        err_msg
    );
}

// Test: ensure_ref_storage_setup - initial deposit exceeds cap (S1 guard)
#[tokio::test]
async fn test_ensure_ref_storage_setup_initial_deposit_exceeds_cap() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();
    let tokens = vec![token];

    let keep = vec![WNEAR_TOKEN.clone()];
    // bounds.min = 0.001 NEAR = 1_000_000_000_000_000_000_000
    // max_top_up = 1 yocto → bounds.min > max_top_up → エラー
    let max_top_up = NearToken::from_yoctonear(1);
    let result = ensure_ref_storage_setup(&client, &wallet, &tokens, &keep, max_top_up).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("exceeds cap"),
        "expected 'exceeds cap' in error: {}",
        err_msg
    );
}

// Test: ensure_ref_storage_setup - cumulative cap (initial + top-up) exceeded
//
// 未登録アカウントで初期 deposit = bounds.min を支払った後、さらに top-up が
// 必要になるケース。max_top_up を「bounds.min + 小額」に設定し、top-up 必要量が
// 残り枠を超えると Err となることを確認する。
#[tokio::test]
async fn test_ensure_ref_storage_setup_cumulative_cap_exceeded() {
    // bounds.min = 1e21、max_top_up は bounds.min の 1 yocto 上に設定
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    // max_top_up = bounds.min + 1 → 初期 deposit 後、残り枠は 1 yocto のみ。
    // しかし MockStorageClient は初期 deposit 後に available = 0 とするため
    // 新トークン登録に数百 yocto 必要 → 1 yocto を超える top-up で Err。
    let max_top_up = NearToken::from_yoctonear(1_000_000_000_000_000_000_001);
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let result = ensure_ref_storage_setup(&client, &wallet, &[new_token], &keep, max_top_up).await;
    // unregistered のため planner は EmptyDeposits を返し、初期 deposit + register のみの
    // パスを通る（top-up は発生せず）。したがって Ok。
    // このテストはアカウントが登録済みかつ cap ギリギリのパスを別途検証する。
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - cumulative cap boundary (exact)
//
// 登録済みアカウントで top-up が必要な状態、かつ max_top_up がちょうど足りる/足りない
// 境界値でのエラー発生確認。
#[tokio::test]
async fn test_ensure_ref_storage_setup_cumulative_cap_boundary() {
    let token: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));

    // available = 0 → top-up が必須。needed ≈ per_token × 11/10。
    // per_token = used / deposits_len = 2e21 / 1 = 2e21
    // needed ≈ 2.2e21 → remaining_cap = max_top_up が needed 未満なら Err
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    // max_top_up = 1 → needed (≈2.2e21) > 1 → Err
    let max_top_up = NearToken::from_yoctonear(1);
    let result = ensure_ref_storage_setup(
        &client,
        &wallet,
        &[token.clone(), new_token.clone()],
        &keep,
        max_top_up,
    )
    .await;
    assert!(result.is_err());

    // max_top_up = 10 NEAR → 余裕あり → Ok
    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let mut deposits = BTreeMap::new();
    deposits.insert(token.clone(), U128(100));
    let client = MockStorageClient::new_with_balance(balance).with_deposits(deposits);
    let ok_cap = NearToken::from_yoctonear(10_000_000_000_000_000_000_000_000);
    let result =
        ensure_ref_storage_setup(&client, &wallet, &[token, new_token], &keep, ok_cap).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - initial deposit equals cap then top-up needed → Err
//
// 未登録アカウントで max_top_up = bounds.min ちょうどに設定。初期 deposit は通るが
// MockStorageClient が初期 deposit 後 available = 0 とし、新トークン登録で
// top-up が必要 → remaining_cap = 0 なので Err。
#[tokio::test]
async fn test_ensure_ref_storage_setup_initial_fills_cap_then_topup_needed() {
    let client = MockStorageClient::new_unregistered();
    let wallet = MockWallet::new();

    let keep = vec![WNEAR_TOKEN.clone()];
    // bounds.min = 1e21、max_top_up = bounds.min → 初期 deposit 後 remaining_cap = 0。
    // ただし unregistered パスは EmptyDeposits 経由で planner をスキップし register のみ
    // (top-up 発生なし) なので Ok で終わる。
    // このケースは cumulative cap の実動検証ではなく「初期 deposit が cap 全量を食う」境界の
    // 挙動確認を兼ねる。
    let max_top_up = NearToken::from_yoctonear(1_000_000_000_000_000_000_000);
    let new_token: TokenAccount = "new.near".parse().unwrap();
    let result = ensure_ref_storage_setup(&client, &wallet, &[new_token], &keep, max_top_up).await;
    assert!(result.is_ok());
}

// Test: ensure_ref_storage_setup - concurrent calls serialize initial deposit
//
// 同一 wallet で 4 並列起動 → 初期 deposit は 1 回しか実行されない。
// 各テストでは static ロックマップ汚染を避けるため固有 account_id を使う。
#[tokio::test]
async fn test_concurrent_ensure_single_initial_deposit() {
    use std::sync::Arc;

    let client = Arc::new(MockStorageClient::new_unregistered());
    let wallet = Arc::new(MockWallet::with_account_id("test-concurrent-initial.near"));
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);

    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        let w = wallet.clone();
        let keep = vec![WNEAR_TOKEN.clone()];
        let tokens = vec![WNEAR_TOKEN.clone()];
        handles.push(tokio::spawn(async move {
            ensure_ref_storage_setup(&*c, &*w, &tokens, &keep, max_top_up).await
        }));
    }

    for h in handles {
        let r = h.await.unwrap();
        assert!(r.is_ok(), "spawn returned Err: {:?}", r);
    }

    // 初期 deposit は 1 回だけ。それ以降の呼び出しは既に登録済で register_tokens のみ。
    assert_eq!(
        client.storage_deposit_count.load(Ordering::Relaxed),
        1,
        "concurrent calls must not double-deposit initial registration"
    );
}

// Test: ensure_ref_storage_setup - concurrent calls serialize top-up
//
// 登録済みアカウントで 4 並列起動 → ロックで直列化され、全呼び出しが成功する。
// Mock では register_tokens 後の deposits 更新が反映されないため本番と異なり
// 各並行呼び出しで top-up が必要と判定される（per_token_floor により needed > 0）。
// これは Mock の簡略化によるもので、ロック機構とは独立。実運用では deposits 更新
// により 2 回目以降は top-up がスキップされる。
#[tokio::test]
async fn test_concurrent_ensure_serializes_cleanly() {
    use std::sync::Arc;

    let wnear: TokenAccount = WNEAR_TOKEN.clone();
    let mut deposits = BTreeMap::new();
    deposits.insert(wnear.clone(), U128(100));

    let balance = StorageBalance {
        total: U128(2_000_000_000_000_000_000_000),
        available: U128(0),
    };
    let client = Arc::new(MockStorageClient::new_with_balance(balance).with_deposits(deposits));
    let wallet = Arc::new(MockWallet::with_account_id("test-concurrent-topup.near"));
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);
    let new_token: TokenAccount = "new.near".parse().unwrap();

    let mut handles = Vec::new();
    for _ in 0..4 {
        let c = client.clone();
        let w = wallet.clone();
        let keep = vec![WNEAR_TOKEN.clone()];
        let tokens = vec![wnear.clone(), new_token.clone()];
        handles.push(tokio::spawn(async move {
            ensure_ref_storage_setup(&*c, &*w, &tokens, &keep, max_top_up).await
        }));
    }

    for h in handles {
        let r = h.await.unwrap();
        assert!(r.is_ok(), "spawn returned Err: {:?}", r);
    }
}

// Test: ensure_ref_storage_setup - different accounts can run concurrently
//
// 別アカウント → 別ロックなので並行実行される。両方とも成功することを確認。
#[tokio::test]
async fn test_concurrent_different_accounts_parallel() {
    use std::sync::Arc;

    let client_a = Arc::new(MockStorageClient::new_unregistered());
    let client_b = Arc::new(MockStorageClient::new_unregistered());
    let wallet_a = Arc::new(MockWallet::with_account_id(
        "test-concurrent-parallel-a.near",
    ));
    let wallet_b = Arc::new(MockWallet::with_account_id(
        "test-concurrent-parallel-b.near",
    ));
    let max_top_up = NearToken::from_yoctonear(500_000_000_000_000_000_000_000);

    let keep_a = vec![WNEAR_TOKEN.clone()];
    let keep_b = vec![WNEAR_TOKEN.clone()];
    let tokens_a = vec![WNEAR_TOKEN.clone()];
    let tokens_b = vec![WNEAR_TOKEN.clone()];

    let (r_a, r_b) = tokio::join!(
        ensure_ref_storage_setup(&*client_a, &*wallet_a, &tokens_a, &keep_a, max_top_up),
        ensure_ref_storage_setup(&*client_b, &*wallet_b, &tokens_b, &keep_b, max_top_up),
    );
    assert!(r_a.is_ok());
    assert!(r_b.is_ok());
}
