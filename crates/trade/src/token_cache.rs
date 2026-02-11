//! トークン decimals のグローバルキャッシュ
//!
//! DB から一括ロードし、キャッシュミス時のみ RPC にフォールバックする。
//! これにより `record_rates` の ~1013 逐次 RPC 呼び出しを 0 に削減する。

use logging::*;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// グローバル decimals キャッシュ: token_id → decimals
static TOKEN_DECIMALS_CACHE: Lazy<RwLock<HashMap<String, u8>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// 起動時に DB から全トークンの decimals を一括ロード (1 SQL クエリ)
pub async fn load_from_db() -> crate::Result<()> {
    let log = DEFAULT.new(o!("function" => "token_cache::load_from_db"));
    trace!(log, "loading token decimals from DB");

    let decimals_map = persistence::token_rate::get_all_decimals().await?;

    let count = decimals_map.len();
    let mut cache = TOKEN_DECIMALS_CACHE.write().await;
    for (token_id, decimals) in decimals_map {
        cache.insert(token_id, decimals);
    }

    info!(log, "loaded token decimals from DB"; "count" => count);
    Ok(())
}

/// キャッシュから decimals を同期的に取得（RPC フォールバックなし）
///
/// キャッシュに存在しない場合は None を返す。
/// シミュレーション等、非同期 RPC を呼べない文脈で使用。
pub fn get_cached_decimals(token_id: &str) -> Option<u8> {
    TOKEN_DECIMALS_CACHE
        .try_read()
        .ok()
        .and_then(|cache| cache.get(token_id).copied())
}

/// 単一トークンの decimals を取得 (キャッシュ優先、ミス時のみ RPC)
///
/// RPC 失敗時はエラーを返し、キャッシュには保存しない。
pub async fn get_token_decimals_cached<C>(client: &C, token_id: &str) -> crate::Result<u8>
where
    C: blockchain::jsonrpc::ViewContract,
{
    // 1. キャッシュから取得
    {
        let cache = TOKEN_DECIMALS_CACHE.read().await;
        if let Some(&decimals) = cache.get(token_id) {
            return Ok(decimals);
        }
    }

    // 2. キャッシュミス: RPC にフォールバック（エラーはキャッシュしない）
    let log = DEFAULT.new(o!(
        "function" => "token_cache::get_token_decimals_cached",
        "token_id" => token_id.to_owned(),
    ));
    debug!(log, "cache miss, fetching decimals via RPC");

    let decimals = super::market_data::get_token_decimals(client, token_id).await?;

    // 3. 成功時のみキャッシュに保存
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(token_id.to_string(), decimals);
    }

    Ok(decimals)
}

/// 複数トークンの decimals を一括確認し、ミス分のみ並列 RPC で取得
pub async fn ensure_decimals_cached<C>(client: &C, token_ids: &[String]) -> HashMap<String, u8>
where
    C: blockchain::jsonrpc::ViewContract + Sync,
{
    let log = DEFAULT.new(o!(
        "function" => "token_cache::ensure_decimals_cached",
        "total_tokens" => token_ids.len(),
    ));

    let mut result = HashMap::new();
    let mut missing = Vec::new();

    // 1. キャッシュから取得
    {
        let cache = TOKEN_DECIMALS_CACHE.read().await;
        for token_id in token_ids {
            if let Some(&decimals) = cache.get(token_id) {
                result.insert(token_id.clone(), decimals);
            } else {
                missing.push(token_id.clone());
            }
        }
    }

    if missing.is_empty() {
        trace!(log, "all tokens found in cache"; "cached" => result.len());
        return result;
    }

    debug!(log, "fetching missing decimals via RPC";
        "cached" => result.len(),
        "missing" => missing.len()
    );

    // 2. ミス分を最大20並列で RPC 取得（失敗分はスキップ）
    use futures::stream::{self, StreamExt};

    let fetched: Vec<(String, Option<u8>)> = stream::iter(missing)
        .map(|token_id| {
            let log = log.clone();
            async move {
                match super::market_data::get_token_decimals(client, &token_id).await {
                    Ok(d) => (token_id, Some(d)),
                    Err(e) => {
                        warn!(log, "failed to fetch decimals via RPC"; "token_id" => &token_id, "error" => %e);
                        (token_id, None)
                    }
                }
            }
        })
        .buffer_unordered(20)
        .collect()
        .await;

    // 3. 成功分のみキャッシュに保存
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        for (token_id, decimals) in &fetched {
            if let Some(d) = decimals {
                cache.insert(token_id.clone(), *d);
            }
        }
    }

    // 4. 成功分のみ結果に追加
    for (token_id, decimals) in fetched {
        if let Some(d) = decimals {
            result.insert(token_id, d);
        }
    }

    trace!(log, "decimals cache updated"; "total" => result.len());
    result
}

#[cfg(test)]
mod tests;
