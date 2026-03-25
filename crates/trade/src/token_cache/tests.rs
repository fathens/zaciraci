use super::*;
use common::config::ConfigResolver;
use serial_test::serial;

const CFG: ConfigResolver = ConfigResolver;

fn ta(s: &str) -> TokenAccount {
    s.parse().unwrap()
}

/// テスト前にキャッシュをクリア（正キャッシュ + ネガティブキャッシュ）
async fn clear_cache() {
    let mut cache = TOKEN_DECIMALS_CACHE.write().await;
    cache.clear();
    let mut failures = TOKEN_DECIMALS_FAILURES.write().await;
    failures.clear();
}

/// ft_metadata を返すモッククライアント（decimals を指定可能）
struct MockMetadataClient {
    decimals: u8,
}

impl blockchain::jsonrpc::ViewContract for MockMetadataClient {
    async fn view_contract<T>(
        &self,
        _receiver: &near_sdk::AccountId,
        method_name: &str,
        _args: &T,
    ) -> crate::Result<near_primitives::views::CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        match method_name {
            "ft_metadata" => {
                let metadata = serde_json::json!({
                    "spec": "ft-1.0.0",
                    "name": "TestToken",
                    "symbol": "TT",
                    "decimals": self.decimals,
                });
                Ok(near_primitives::views::CallResult {
                    result: serde_json::to_vec(&metadata).unwrap(),
                    logs: vec![],
                })
            }
            _ => Err(anyhow::anyhow!("Unexpected method: {}", method_name)),
        }
    }
}

/// 特定トークンのみ成功し、それ以外はエラーを返すモッククライアント
struct PartialFailClient {
    /// 成功するトークン → decimals のマップ
    success_map: std::collections::HashMap<String, u8>,
}

impl blockchain::jsonrpc::ViewContract for PartialFailClient {
    async fn view_contract<T>(
        &self,
        receiver: &near_sdk::AccountId,
        method_name: &str,
        _args: &T,
    ) -> crate::Result<near_primitives::views::CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        let token_id = receiver.to_string();
        match (method_name, self.success_map.get(&token_id)) {
            ("ft_metadata", Some(&decimals)) => {
                let metadata = serde_json::json!({
                    "spec": "ft-1.0.0",
                    "name": "TestToken",
                    "symbol": "TT",
                    "decimals": decimals,
                });
                Ok(near_primitives::views::CallResult {
                    result: serde_json::to_vec(&metadata).unwrap(),
                    logs: vec![],
                })
            }
            _ => Err(anyhow::anyhow!("RPC error for {}", token_id)),
        }
    }
}

/// 常にエラーを返すモッククライアント
struct FailingClient;

impl blockchain::jsonrpc::ViewContract for FailingClient {
    async fn view_contract<T>(
        &self,
        _receiver: &near_sdk::AccountId,
        _method_name: &str,
        _args: &T,
    ) -> crate::Result<near_primitives::views::CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        Err(anyhow::anyhow!("RPC connection failed"))
    }
}

// --- get_token_decimals_cached ---

#[tokio::test]
#[serial]
async fn test_get_cached_returns_cached_value() {
    clear_cache().await;

    // キャッシュにプリロード
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(ta("token.near"), 18);
    }

    // RPC の decimals=99 は呼ばれないはず
    let client = MockMetadataClient { decimals: 99 };
    let result = get_token_decimals_cached(&client, &ta("token.near"))
        .await
        .unwrap();
    assert_eq!(result, 18);
}

#[tokio::test]
#[serial]
async fn test_get_cached_falls_back_to_rpc_on_miss() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 6 };
    let result = get_token_decimals_cached(&client, &ta("usdt.near"))
        .await
        .unwrap();
    assert_eq!(result, 6);

    // RPC 結果がキャッシュに保存されている
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.get(&ta("usdt.near")), Some(&6));
}

#[tokio::test]
#[serial]
async fn test_get_cached_rpc_result_persists_in_cache() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 8 };

    // 1回目: RPC フォールバック
    let first = get_token_decimals_cached(&client, &ta("dai.near"))
        .await
        .unwrap();
    assert_eq!(first, 8);

    // 2回目: キャッシュから取得（別の decimals のクライアントでも同じ値が返る）
    let client2 = MockMetadataClient { decimals: 99 };
    let second = get_token_decimals_cached(&client2, &ta("dai.near"))
        .await
        .unwrap();
    assert_eq!(second, 8); // キャッシュ値
}

#[tokio::test]
#[serial]
async fn test_get_cached_rpc_failure_returns_error() {
    clear_cache().await;

    let client = FailingClient;
    let result = get_token_decimals_cached(&client, &ta("fail.near")).await;
    assert!(result.is_err());

    // エラー時はキャッシュに保存されない
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert!(cache.get(&ta("fail.near")).is_none());
}

#[tokio::test]
#[serial]
async fn test_get_cached_rpc_failure_then_success_caches_correct_value() {
    clear_cache().await;

    // 1回目: RPC 失敗 → エラー、キャッシュ未保存
    let failing_client = FailingClient;
    let result = get_token_decimals_cached(&failing_client, &ta("retry.near")).await;
    assert!(result.is_err());
    {
        let cache = TOKEN_DECIMALS_CACHE.read().await;
        assert!(cache.get(&ta("retry.near")).is_none());
    }

    // 2回目: RPC 成功 → キャッシュに保存
    let client = MockMetadataClient { decimals: 6 };
    let result = get_token_decimals_cached(&client, &ta("retry.near"))
        .await
        .unwrap();
    assert_eq!(result, 6);
    {
        let cache = TOKEN_DECIMALS_CACHE.read().await;
        assert_eq!(cache.get(&ta("retry.near")), Some(&6));
    }
}

// --- ensure_decimals_cached ---

#[tokio::test]
#[serial]
async fn test_ensure_all_cached() {
    clear_cache().await;

    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(ta("a.near"), 18);
        cache.insert(ta("b.near"), 24);
    }

    let client = MockMetadataClient { decimals: 99 };
    let token_ids = vec![ta("a.near"), ta("b.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    assert_eq!(result.len(), 2);
    assert_eq!(result[&ta("a.near")], 18);
    assert_eq!(result[&ta("b.near")], 24);
}

#[tokio::test]
#[serial]
async fn test_ensure_fetches_missing_via_rpc() {
    clear_cache().await;

    // 一部だけキャッシュにプリロード
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(ta("cached.near"), 18);
    }

    let client = MockMetadataClient { decimals: 8 };
    let token_ids = vec![ta("cached.near"), ta("missing.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    assert_eq!(result.len(), 2);
    assert_eq!(result[&ta("cached.near")], 18); // キャッシュから
    assert_eq!(result[&ta("missing.near")], 8); // RPC から

    // missing.near もキャッシュに保存された
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.get(&ta("missing.near")), Some(&8));
}

#[tokio::test]
#[serial]
async fn test_ensure_all_missing() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 12 };
    let token_ids = vec![ta("x.near"), ta("y.near"), ta("z.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    assert_eq!(result.len(), 3);
    assert_eq!(result[&ta("x.near")], 12);
    assert_eq!(result[&ta("y.near")], 12);
    assert_eq!(result[&ta("z.near")], 12);

    // 全てキャッシュに保存された
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.len(), 3);
}

#[tokio::test]
#[serial]
async fn test_ensure_empty_input() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 99 };
    let result = ensure_decimals_cached(&client, &[], &CFG).await;
    assert!(result.is_empty());
}

#[tokio::test]
#[serial]
async fn test_ensure_duplicate_tokens_deduplicated_by_caller() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 6 };
    // 呼び出し元で重複排除される前提だが、重複入力でもパニックしないことを確認
    let token_ids = vec![ta("dup.near"), ta("dup.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    // HashMap なので最終的に1エントリ
    assert_eq!(result.len(), 1);
    assert_eq!(result[&ta("dup.near")], 6);
}

#[tokio::test]
#[serial]
async fn test_ensure_rpc_failure_skips_token() {
    clear_cache().await;

    // RPC が常に失敗するクライアント
    let client = FailingClient;
    let token_ids = vec![ta("fail1.near"), ta("fail2.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    // RPC 失敗トークンは結果に含まれない
    assert!(result.is_empty());

    // 正キャッシュにも保存されない
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert!(cache.get(&ta("fail1.near")).is_none());
    assert!(cache.get(&ta("fail2.near")).is_none());
    drop(cache);

    // ネガティブキャッシュに記録される
    let failures = TOKEN_DECIMALS_FAILURES.read().await;
    assert!(failures.contains_key(&ta("fail1.near")));
    assert!(failures.contains_key(&ta("fail2.near")));
    assert_eq!(failures[&ta("fail1.near")].failure_count, 1);
    assert_eq!(failures[&ta("fail2.near")].failure_count, 1);
}

#[tokio::test]
#[serial]
async fn test_ensure_partial_rpc_failure_returns_only_successful() {
    clear_cache().await;

    // ok.near → decimals=6 (成功), fail.near → エラー (失敗)
    let mut success_map = std::collections::HashMap::new();
    success_map.insert("ok.near".to_string(), 6u8);
    let client = PartialFailClient { success_map };

    let token_ids = vec![ta("ok.near"), ta("fail.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    // 成功分のみ結果に含まれる
    assert_eq!(result.len(), 1);
    assert_eq!(result[&ta("ok.near")], 6);
    assert!(!result.contains_key(&ta("fail.near")));

    // 成功分のみキャッシュに保存される
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.get(&ta("ok.near")), Some(&6));
    assert!(cache.get(&ta("fail.near")).is_none());
}

// --- negative cache (backoff) ---

#[tokio::test]
#[serial]
async fn test_negative_cache_skips_recently_failed_token() {
    clear_cache().await;

    let client = FailingClient;
    let token_ids = vec![ta("bad.near")];

    // 1回目: RPC 失敗 → ネガティブキャッシュに記録
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;
    assert!(result.is_empty());

    {
        let failures = TOKEN_DECIMALS_FAILURES.read().await;
        assert_eq!(failures[&ta("bad.near")].failure_count, 1);
    }

    // 2回目: バックオフ中なので RPC は呼ばれずスキップ
    // (base=15分なので、即座の再試行はスキップされる)
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;
    assert!(result.is_empty());

    // failure_count は増えない（RPC 呼出自体がスキップされるため）
    let failures = TOKEN_DECIMALS_FAILURES.read().await;
    assert_eq!(failures[&ta("bad.near")].failure_count, 1);
}

#[tokio::test]
#[serial]
async fn test_negative_cache_retries_after_backoff_expires() {
    clear_cache().await;

    // ネガティブキャッシュに古い失敗記録を手動で挿入
    {
        let mut failures = TOKEN_DECIMALS_FAILURES.write().await;
        failures.insert(
            ta("recover.near"),
            FailureRecord {
                failure_count: 1,
                // 30分前の失敗 → base=15分の backoff(2^1=30分) で期限ギリギリ
                last_failure: Utc::now() - chrono::TimeDelta::minutes(31),
            },
        );
    }

    // RPC 成功するクライアント
    let client = MockMetadataClient { decimals: 18 };
    let token_ids = vec![ta("recover.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;

    // バックオフ期限切れ → RPC 呼出 → 成功
    assert_eq!(result.len(), 1);
    assert_eq!(result[&ta("recover.near")], 18);

    // ネガティブキャッシュから削除された
    let failures = TOKEN_DECIMALS_FAILURES.read().await;
    assert!(!failures.contains_key(&ta("recover.near")));
}

#[tokio::test]
#[serial]
async fn test_negative_cache_increments_failure_count() {
    clear_cache().await;

    // ネガティブキャッシュに期限切れの失敗記録を挿入
    {
        let mut failures = TOKEN_DECIMALS_FAILURES.write().await;
        failures.insert(
            ta("stubborn.near"),
            FailureRecord {
                failure_count: 2,
                // 十分古い → リトライ対象
                last_failure: Utc::now() - chrono::TimeDelta::hours(24),
            },
        );
    }

    // RPC は再び失敗
    let client = FailingClient;
    let token_ids = vec![ta("stubborn.near")];
    let result = ensure_decimals_cached(&client, &token_ids, &CFG).await;
    assert!(result.is_empty());

    // failure_count がインクリメントされた
    let failures = TOKEN_DECIMALS_FAILURES.read().await;
    assert_eq!(failures[&ta("stubborn.near")].failure_count, 3);
}

#[tokio::test]
#[serial]
async fn test_cleanup_stale_failures_removes_inactive_tokens() {
    clear_cache().await;

    // ネガティブキャッシュに複数トークンを登録
    {
        let mut failures = TOKEN_DECIMALS_FAILURES.write().await;
        for token in &["active.near", "stale1.near", "stale2.near"] {
            failures.insert(
                ta(token),
                FailureRecord {
                    failure_count: 1,
                    last_failure: Utc::now(),
                },
            );
        }
    }

    // active.near のみアクティブ
    let active = vec![ta("active.near"), ta("other.near")];
    cleanup_stale_failures(&active).await;

    let failures = TOKEN_DECIMALS_FAILURES.read().await;
    assert!(failures.contains_key(&ta("active.near")));
    assert!(!failures.contains_key(&ta("stale1.near")));
    assert!(!failures.contains_key(&ta("stale2.near")));
}

#[tokio::test]
#[serial]
async fn test_failure_record_should_retry_backoff_progression() {
    let now = Utc::now();
    let base = 15u64;
    let max = 1440u64;

    let record = FailureRecord {
        failure_count: 1,
        last_failure: now,
    };

    // failure_count=1 → backoff = 15 * 2^1 = 30分
    assert!(!record.should_retry(now, base, max)); // 0分経過
    assert!(!record.should_retry(now + chrono::TimeDelta::minutes(29), base, max)); // 29分経過
    assert!(record.should_retry(now + chrono::TimeDelta::minutes(30), base, max)); // 30分経過

    let record3 = FailureRecord {
        failure_count: 3,
        last_failure: now,
    };

    // failure_count=3 → backoff = 15 * 2^3 = 120分
    assert!(!record3.should_retry(now + chrono::TimeDelta::minutes(119), base, max));
    assert!(record3.should_retry(now + chrono::TimeDelta::minutes(120), base, max));

    let record_high = FailureRecord {
        failure_count: 20,
        last_failure: now,
    };

    // failure_count=20 → exponent capped at 8, 15 * 256 = 3840 → max_backoff=1440
    assert!(!record_high.should_retry(now + chrono::TimeDelta::minutes(1439), base, max));
    assert!(record_high.should_retry(now + chrono::TimeDelta::minutes(1440), base, max));
}
