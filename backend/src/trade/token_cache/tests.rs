use super::*;
use serial_test::serial;

/// テスト前にキャッシュをクリア
async fn clear_cache() {
    let mut cache = TOKEN_DECIMALS_CACHE.write().await;
    cache.clear();
}

/// ft_metadata を返すモッククライアント（decimals を指定可能）
struct MockMetadataClient {
    decimals: u8,
}

impl crate::jsonrpc::ViewContract for MockMetadataClient {
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

// --- get_token_decimals_cached ---

#[tokio::test]
#[serial]
async fn test_get_cached_returns_cached_value() {
    clear_cache().await;

    // キャッシュにプリロード
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert("token.near".to_string(), 18);
    }

    // RPC の decimals=99 は呼ばれないはず
    let client = MockMetadataClient { decimals: 99 };
    let result = get_token_decimals_cached(&client, "token.near").await;
    assert_eq!(result, 18);
}

#[tokio::test]
#[serial]
async fn test_get_cached_falls_back_to_rpc_on_miss() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 6 };
    let result = get_token_decimals_cached(&client, "usdt.near").await;
    assert_eq!(result, 6);

    // RPC 結果がキャッシュに保存されている
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.get("usdt.near"), Some(&6));
}

#[tokio::test]
#[serial]
async fn test_get_cached_rpc_result_persists_in_cache() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 8 };

    // 1回目: RPC フォールバック
    let first = get_token_decimals_cached(&client, "dai.near").await;
    assert_eq!(first, 8);

    // 2回目: キャッシュから取得（別の decimals のクライアントでも同じ値が返る）
    let client2 = MockMetadataClient { decimals: 99 };
    let second = get_token_decimals_cached(&client2, "dai.near").await;
    assert_eq!(second, 8); // キャッシュ値
}

// --- ensure_decimals_cached ---

#[tokio::test]
#[serial]
async fn test_ensure_all_cached() {
    clear_cache().await;

    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert("a.near".to_string(), 18);
        cache.insert("b.near".to_string(), 24);
    }

    let client = MockMetadataClient { decimals: 99 };
    let token_ids = vec!["a.near".to_string(), "b.near".to_string()];
    let result = ensure_decimals_cached(&client, &token_ids).await;

    assert_eq!(result.len(), 2);
    assert_eq!(result["a.near"], 18);
    assert_eq!(result["b.near"], 24);
}

#[tokio::test]
#[serial]
async fn test_ensure_fetches_missing_via_rpc() {
    clear_cache().await;

    // 一部だけキャッシュにプリロード
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert("cached.near".to_string(), 18);
    }

    let client = MockMetadataClient { decimals: 8 };
    let token_ids = vec!["cached.near".to_string(), "missing.near".to_string()];
    let result = ensure_decimals_cached(&client, &token_ids).await;

    assert_eq!(result.len(), 2);
    assert_eq!(result["cached.near"], 18); // キャッシュから
    assert_eq!(result["missing.near"], 8); // RPC から

    // missing.near もキャッシュに保存された
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.get("missing.near"), Some(&8));
}

#[tokio::test]
#[serial]
async fn test_ensure_all_missing() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 12 };
    let token_ids = vec![
        "x.near".to_string(),
        "y.near".to_string(),
        "z.near".to_string(),
    ];
    let result = ensure_decimals_cached(&client, &token_ids).await;

    assert_eq!(result.len(), 3);
    assert_eq!(result["x.near"], 12);
    assert_eq!(result["y.near"], 12);
    assert_eq!(result["z.near"], 12);

    // 全てキャッシュに保存された
    let cache = TOKEN_DECIMALS_CACHE.read().await;
    assert_eq!(cache.len(), 3);
}

#[tokio::test]
#[serial]
async fn test_ensure_empty_input() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 99 };
    let result = ensure_decimals_cached(&client, &[]).await;
    assert!(result.is_empty());
}

#[tokio::test]
#[serial]
async fn test_ensure_duplicate_tokens_deduplicated_by_caller() {
    clear_cache().await;

    let client = MockMetadataClient { decimals: 6 };
    // 呼び出し元で重複排除される前提だが、重複入力でもパニックしないことを確認
    let token_ids = vec!["dup.near".to_string(), "dup.near".to_string()];
    let result = ensure_decimals_cached(&client, &token_ids).await;

    // HashMap なので最終的に1エントリ
    assert_eq!(result.len(), 1);
    assert_eq!(result["dup.near"], 6);
}
