//! トークン decimals のグローバルキャッシュ
//!
//! DB から一括ロードし、キャッシュミス時のみ RPC にフォールバックする。
//! これにより `record_rates` の ~1013 逐次 RPC 呼び出しを 0 に削減する。

use crate::logging::*;
use crate::persistence::connection_pool;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use tokio::sync::RwLock;

/// グローバル decimals キャッシュ: token_id → decimals
static TOKEN_DECIMALS_CACHE: Lazy<RwLock<HashMap<String, u8>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// DB クエリ結果用の構造体
#[derive(Debug, Clone, diesel::QueryableByName)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct TokenDecimalsRow {
    #[diesel(sql_type = diesel::sql_types::Text)]
    base_token: String,
    #[diesel(sql_type = diesel::sql_types::SmallInt)]
    decimals: i16,
}

/// 起動時に DB から全トークンの decimals を一括ロード (1 SQL クエリ)
pub async fn load_from_db() -> crate::Result<()> {
    let log = DEFAULT.new(o!("function" => "token_cache::load_from_db"));
    info!(log, "loading token decimals from DB");

    let conn = connection_pool::get().await?;

    let rows: Vec<TokenDecimalsRow> = conn
        .interact(|conn| {
            use diesel::RunQueryDsl;
            diesel::sql_query(
                "SELECT DISTINCT ON (base_token) base_token, decimals \
                 FROM token_rates \
                 WHERE decimals IS NOT NULL \
                 ORDER BY base_token, timestamp DESC",
            )
            .load::<TokenDecimalsRow>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    let count = rows.len();
    let mut cache = TOKEN_DECIMALS_CACHE.write().await;
    for row in rows {
        cache.insert(row.base_token, row.decimals as u8);
    }

    info!(log, "loaded token decimals from DB"; "count" => count);
    Ok(())
}

/// 単一トークンの decimals を取得 (キャッシュ優先、ミス時のみ RPC)
pub async fn get_token_decimals_cached<C>(client: &C, token_id: &str) -> u8
where
    C: crate::jsonrpc::ViewContract,
{
    // 1. キャッシュから取得
    {
        let cache = TOKEN_DECIMALS_CACHE.read().await;
        if let Some(&decimals) = cache.get(token_id) {
            return decimals;
        }
    }

    // 2. キャッシュミス: RPC にフォールバック
    let log = DEFAULT.new(o!(
        "function" => "token_cache::get_token_decimals_cached",
        "token_id" => token_id.to_owned(),
    ));
    info!(log, "cache miss, fetching decimals via RPC");

    let decimals = super::stats::get_token_decimals(client, token_id).await;

    // 3. キャッシュに保存
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(token_id.to_string(), decimals);
    }

    decimals
}

/// 複数トークンの decimals を一括確認し、ミス分のみ並列 RPC で取得
pub async fn ensure_decimals_cached<C>(client: &C, token_ids: &[String]) -> HashMap<String, u8>
where
    C: crate::jsonrpc::ViewContract + Sync,
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
        info!(log, "all tokens found in cache"; "cached" => result.len());
        return result;
    }

    info!(log, "fetching missing decimals via RPC";
        "cached" => result.len(),
        "missing" => missing.len()
    );

    // 2. ミス分を最大20並列で RPC 取得
    use futures::stream::{self, StreamExt};

    let fetched: Vec<(String, u8)> = stream::iter(missing)
        .map(|token_id| async move {
            let decimals = super::stats::get_token_decimals(client, &token_id).await;
            (token_id, decimals)
        })
        .buffer_unordered(20)
        .collect()
        .await;

    // 3. キャッシュに保存
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        for (token_id, decimals) in &fetched {
            cache.insert(token_id.clone(), *decimals);
        }
    }

    // 4. 結果に追加
    for (token_id, decimals) in fetched {
        result.insert(token_id, decimals);
    }

    info!(log, "decimals cache updated"; "total" => result.len());
    result
}

#[cfg(test)]
mod tests;
