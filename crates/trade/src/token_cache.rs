//! トークン decimals のグローバルキャッシュ
//!
//! DB から一括ロードし、キャッシュミス時のみ RPC にフォールバックする。
//! これにより `record_rates` の ~1013 逐次 RPC 呼び出しを 0 に削減する。
//!
//! RPC 取得に失敗したトークンはネガティブキャッシュに記録し、
//! 指数バックオフで再試行頻度を自動的に低下させる。

use chrono::{DateTime, Utc};
use common::config::ConfigAccess;
use common::types::TokenAccount;
use logging::*;
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::RwLock;

/// グローバル decimals キャッシュ: token_id → decimals
static TOKEN_DECIMALS_CACHE: LazyLock<RwLock<HashMap<TokenAccount, u8>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// RPC 取得失敗の記録
struct FailureRecord {
    /// 連続失敗回数
    failure_count: u32,
    /// 最終失敗時刻
    last_failure: DateTime<Utc>,
}

impl FailureRecord {
    /// バックオフ期間が経過し、再試行すべきかを判定
    ///
    /// backoff = base_minutes * 2^min(failure_count, 8), capped at max_backoff_minutes
    fn should_retry(
        &self,
        now: DateTime<Utc>,
        base_minutes: u64,
        max_backoff_minutes: u64,
    ) -> bool {
        let exponent = self.failure_count.min(8);
        let backoff_minutes = base_minutes
            .saturating_mul(2u64.saturating_pow(exponent))
            .min(max_backoff_minutes);

        let elapsed = now.signed_duration_since(self.last_failure);
        elapsed >= chrono::TimeDelta::minutes(backoff_minutes as i64)
    }
}

/// ネガティブキャッシュ: RPC 取得に失敗したトークンの記録
static TOKEN_DECIMALS_FAILURES: LazyLock<RwLock<HashMap<TokenAccount, FailureRecord>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 起動時に DB から全トークンの decimals を一括ロード (1 SQL クエリ)
pub async fn load_from_db() -> crate::Result<()> {
    let log = DEFAULT.new(o!("function" => "token_cache::load_from_db"));
    trace!(log, "loading token decimals from DB");

    let decimals_map = persistence::token_rate::get_all_decimals().await?;

    let count = decimals_map.len();
    let mut cache = TOKEN_DECIMALS_CACHE.write().await;
    *cache = decimals_map;

    info!(log, "loaded token decimals from DB"; "count" => count);
    Ok(())
}

/// キャッシュから decimals を同期的に取得（RPC フォールバックなし）
///
/// キャッシュに存在しない場合は None を返す。
/// シミュレーション等、非同期 RPC を呼べない文脈で使用。
pub fn get_cached_decimals(token_id: &TokenAccount) -> Option<u8> {
    TOKEN_DECIMALS_CACHE
        .try_read()
        .ok()
        .and_then(|cache| cache.get(token_id).copied())
}

/// 単一トークンの decimals を取得 (キャッシュ優先、ミス時のみ RPC)
///
/// RPC 失敗時はエラーを返し、キャッシュには保存しない。
pub async fn get_token_decimals_cached<C>(client: &C, token_id: &TokenAccount) -> crate::Result<u8>
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
        "token_id" => token_id.to_string(),
    ));
    debug!(log, "cache miss, fetching decimals via RPC");

    let decimals = super::market_data::get_token_decimals(client, token_id).await?;

    // 3. 成功時のみキャッシュに保存
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        cache.insert(token_id.clone(), decimals);
    }

    Ok(decimals)
}

/// 複数トークンの decimals を一括確認し、ミス分のみ並列 RPC で取得
///
/// ネガティブキャッシュにより、繰り返し失敗するトークンは指数バックオフで
/// 再試行頻度を自動的に低下させる。
pub async fn ensure_decimals_cached<C>(
    client: &C,
    token_ids: &[TokenAccount],
    cfg: &impl ConfigAccess,
) -> HashMap<TokenAccount, u8>
where
    C: blockchain::jsonrpc::ViewContract + Sync,
{
    let log = DEFAULT.new(o!(
        "function" => "token_cache::ensure_decimals_cached",
        "total_tokens" => token_ids.len(),
    ));

    let mut result = HashMap::new();
    let mut missing = Vec::new();

    // 1. 正キャッシュから取得
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

    // 2. ネガティブキャッシュでフィルタ: バックオフ中のトークンをスキップ
    let now = Utc::now();
    let base_minutes = cfg.trade_token_cache_backoff_base_minutes();
    let max_backoff = cfg.trade_token_cache_max_backoff_minutes();
    let mut backoff_skipped: usize = 0;

    {
        let failures = TOKEN_DECIMALS_FAILURES.read().await;
        missing.retain(|token_id| {
            if let Some(record) = failures.get(token_id) {
                if record.should_retry(now, base_minutes, max_backoff) {
                    true
                } else {
                    backoff_skipped += 1;
                    false
                }
            } else {
                true
            }
        });
    }

    if missing.is_empty() {
        debug!(log, "all missing tokens in backoff";
            "cached" => result.len(),
            "backoff_skipped" => backoff_skipped
        );
        return result;
    }

    debug!(log, "fetching missing decimals via RPC";
        "cached" => result.len(),
        "to_fetch" => missing.len(),
        "backoff_skipped" => backoff_skipped
    );

    // 3. ミス分を並列で RPC 取得（失敗分はスキップ）
    use futures::stream::{self, StreamExt};

    let concurrency = cfg.trade_token_cache_concurrency() as usize;

    let fetched: Vec<(TokenAccount, std::result::Result<u8, String>)> = stream::iter(missing)
        .map(|token_id| {
            let log = log.clone();
            async move {
                match super::market_data::get_token_decimals(client, &token_id).await {
                    Ok(d) => (token_id, Ok(d)),
                    Err(e) => {
                        warn!(log, "failed to fetch decimals via RPC"; "token_id" => %token_id, "error" => %e);
                        (token_id, Err(e.to_string()))
                    }
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // 4. 正キャッシュ + ネガティブキャッシュを更新
    let mut newly_failed: usize = 0;
    let total_tracked_failures: usize;
    {
        let mut cache = TOKEN_DECIMALS_CACHE.write().await;
        let mut failures = TOKEN_DECIMALS_FAILURES.write().await;

        for (token_id, fetch_result) in &fetched {
            match fetch_result {
                Ok(d) => {
                    cache.insert(token_id.clone(), *d);
                    // 成功時はネガティブキャッシュから削除（復帰）
                    failures.remove(token_id);
                }
                Err(_) => {
                    newly_failed += 1;
                    let record = failures.entry(token_id.clone()).or_insert(FailureRecord {
                        failure_count: 0,
                        last_failure: now,
                    });
                    record.failure_count += 1;
                    record.last_failure = now;
                }
            }
        }
        total_tracked_failures = failures.len();
    }

    // 5. 成功分のみ結果に追加
    for (token_id, fetch_result) in fetched {
        if let Ok(d) = fetch_result {
            result.insert(token_id, d);
        }
    }

    if backoff_skipped > 0 || newly_failed > 0 {
        info!(log, "decimals fetch summary";
            "cached" => result.len(),
            "newly_failed" => newly_failed,
            "backoff_skipped" => backoff_skipped,
            "total_tracked_failures" => total_tracked_failures,
        );
    }

    trace!(log, "decimals cache updated"; "total" => result.len());
    result
}

/// プールに存在しないトークンの古い失敗記録を削除
pub async fn cleanup_stale_failures(active_token_ids: &[TokenAccount]) {
    let active_set: std::collections::HashSet<_> = active_token_ids.iter().collect();
    let mut failures = TOKEN_DECIMALS_FAILURES.write().await;
    failures.retain(|token_id, _| active_set.contains(token_id));
}

#[cfg(test)]
mod tests;
