pub mod harvest;
pub mod predict;
pub mod recorder;
pub mod stats;
pub mod swap;
pub mod token_cache;

// Re-export algorithm from common crate for backward compatibility
// pub use zaciraci_common::algorithm;

use crate::Result;
use crate::config;
use crate::jsonrpc;
use crate::logging::*;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance;
use crate::ref_finance::token_account::TokenInAccount;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use bigdecimal::BigDecimal;
use chrono::Utc as TZ;
use std::future::Future;
use std::sync::Arc;
use zaciraci_common::types::NearAmount;
use zaciraci_common::types::TokenAmount;

pub async fn run() {
    // DB からトークン decimals キャッシュを初期化
    if let Err(e) = token_cache::load_from_db().await {
        let log = DEFAULT.new(o!("function" => "run"));
        error!(log, "failed to load token decimals cache from DB"; "error" => ?e);
    }

    tokio::spawn(run_record_rates());
    tokio::spawn(run_trade());
}

/// 環境変数から cron スケジュールを取得してパースする
fn get_cron_schedule(env_var: &str, default: &str) -> cron::Schedule {
    let log = DEFAULT.new(o!("function" => "get_cron_schedule", "env_var" => env_var.to_owned()));
    let cron_conf = config::get(env_var).unwrap_or_else(|_| default.to_string());

    match cron_conf.parse() {
        Ok(s) => {
            info!(log, "cron schedule configured"; "schedule" => &cron_conf);
            s
        }
        Err(e) => {
            error!(log, "failed to parse cron schedule, using default";
                   "error" => ?e, "schedule" => &cron_conf, "default" => default);
            default.parse().unwrap()
        }
    }
}

async fn run_record_rates() {
    const DEFAULT_CRON: &str = "0 */15 * * * *"; // デフォルト: 15分間隔
    let schedule = get_cron_schedule("RECORD_RATES_CRON_SCHEDULE", DEFAULT_CRON);
    cronjob(schedule, record_rates, "record_rates").await;
}

async fn run_trade() {
    const DEFAULT_CRON: &str = "0 0 0 * * *"; // デフォルト: 毎日午前0時
    let log = DEFAULT.new(o!("function" => "run_trade"));
    info!(log, "initializing auto trade cron job");

    let schedule = get_cron_schedule("TRADE_CRON_SCHEDULE", DEFAULT_CRON);
    cronjob(schedule, stats::start, "auto_trade").await;
}

async fn cronjob<F, Fut>(schedule: cron::Schedule, func: F, name: &str)
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let log = DEFAULT.new(o!("function" => "cronjob", "name" => name.to_owned()));
    info!(log, "starting cron job");

    for (iteration, next) in schedule.upcoming(TZ).enumerate() {
        let now = TZ::now();
        debug!(log, "cron iteration"; "iteration" => iteration, "next" => %next, "now" => %now);

        // 実行時刻を過ぎている場合はスキップ
        if next <= now {
            warn!(log, "execution time already passed, skipping to next iteration";
                "next" => %next,
                "now" => %now,
                "iteration" => iteration
            );
            continue;
        }

        match (next - now).to_std() {
            Ok(wait) => {
                info!(log, "waiting for next execution";
                    "wait_seconds" => wait.as_secs(),
                    "next_time" => %next
                );

                // 長時間sleepを避けるため、1分間隔でチェック
                loop {
                    let now = TZ::now();
                    if now >= next {
                        break;
                    }

                    let remaining = match (next - now).to_std() {
                        Ok(d) => d,
                        Err(_) => break, // 時刻が過去になった場合は即座に実行
                    };

                    // 最大1分間sleep（残り時間が1分未満なら残り時間）
                    let sleep_duration = remaining.min(std::time::Duration::from_secs(60));

                    // 長時間待機の場合は定期的にログを出力（5分以上待機時のみ）
                    if remaining.as_secs() > 300 {
                        debug!(log, "still waiting for next execution";
                            "remaining_seconds" => remaining.as_secs(),
                            "next_time" => %next
                        );
                    }

                    tokio::time::sleep(sleep_duration).await;
                }

                let exec_log = DEFAULT.new(o!("function" => "run", "name" => name.to_owned()));
                info!(exec_log, "executing scheduled task");

                match func().await {
                    Ok(_) => info!(exec_log, "success"),
                    Err(err) => error!(exec_log, "failure"; "error" => ?err),
                }
            }
            Err(e) => {
                error!(log, "failed to calculate wait duration";
                    "error" => ?e,
                    "next" => %next,
                    "now" => %now,
                    "iteration" => iteration
                );
            }
        }
    }
}

fn get_quote_token() -> TokenInAccount {
    WNEAR_TOKEN.to_in()
}

fn get_initial_value() -> NearAmount {
    config::get("CRON_RECORD_RATES_INITIAL_VALUE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| "100".parse().unwrap())
}

async fn record_rates() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_rates"));

    let quote_token = &get_quote_token();
    let initial_value = get_initial_value();

    info!(log, "recording rates";
        "quote_token" => %quote_token,
        "initial_value" => %initial_value,
    );

    let client = &jsonrpc::new_client();

    info!(log, "loading pools");
    let pools = ref_finance::pool_info::PoolInfoList::read_from_node(client).await?;
    pools.write_to_db().await?;

    info!(log, "updating graph");
    let graph = ref_finance::path::graph::TokenGraph::new(Arc::clone(&pools));
    let goals = graph.update_graph(quote_token)?;
    info!(log, "found targets"; "goals" => %goals.len());
    // NearAmount → YoctoAmount → u128 に変換して list_values に渡す
    let initial_yocto = initial_value.to_yocto().to_u128();
    let values = graph.list_values(initial_yocto, quote_token, &goals)?;

    let log = log.new(o!(
        "num_values" => values.len().to_string(),
    ));

    info!(log, "converting to rates (yocto scale)");

    // 各トークンの decimals を取得（キャッシュ経由、DB 初期化済み）
    let token_ids: Vec<String> = values
        .iter()
        .map(|(base, _)| base.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let token_decimals = token_cache::ensure_decimals_cached(client, &token_ids).await;

    // ExchangeRate を使って TokenRate を構築
    // TokenAmount / NearAmount → ExchangeRate で型安全に rate を計算
    let rates: Vec<_> = values
        .into_iter()
        .filter_map(|(base, value)| {
            let token_str = base.to_string();
            match token_decimals.get(&token_str).copied() {
                Some(decimals) => {
                    let amount =
                        TokenAmount::from_smallest_units(BigDecimal::from(value), decimals);
                    let exchange_rate = &amount / &initial_value;
                    Some(TokenRate::new(base, quote_token.clone(), exchange_rate))
                }
                None => {
                    warn!(log, "skipping token: decimals unknown"; "token" => &token_str);
                    None
                }
            }
        })
        .collect();

    info!(log, "inserting rates");
    TokenRate::batch_insert(&rates).await?;

    info!(log, "success");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_cron_schedule_uses_default_when_env_not_set() {
        // 存在しない環境変数名を使用
        let schedule = get_cron_schedule("TEST_NONEXISTENT_CRON_VAR", "0 */15 * * * *");
        let mut upcoming = schedule.upcoming(TZ);
        let first = upcoming.next().unwrap();
        let second = upcoming.next().unwrap();
        assert_eq!((second - first).num_minutes(), 15);
    }

    #[test]
    fn test_get_cron_schedule_uses_env_value() {
        // SAFETY: テスト専用の環境変数名を使用しているため競合しない
        unsafe {
            std::env::set_var("TEST_CRON_VALID", "0 */30 * * * *");
        }
        let schedule = get_cron_schedule("TEST_CRON_VALID", "0 */15 * * * *");
        let mut upcoming = schedule.upcoming(TZ);
        let first = upcoming.next().unwrap();
        let second = upcoming.next().unwrap();
        assert_eq!((second - first).num_minutes(), 30); // 環境変数の値が使われる
        unsafe {
            std::env::remove_var("TEST_CRON_VALID");
        }
    }

    #[test]
    fn test_get_cron_schedule_fallback_on_invalid_env() {
        // SAFETY: テスト専用の環境変数名を使用しているため競合しない
        unsafe {
            std::env::set_var("TEST_CRON_INVALID", "invalid cron");
        }
        let schedule = get_cron_schedule("TEST_CRON_INVALID", "0 */15 * * * *");
        let mut upcoming = schedule.upcoming(TZ);
        let first = upcoming.next().unwrap();
        let second = upcoming.next().unwrap();
        assert_eq!((second - first).num_minutes(), 15); // デフォルトにフォールバック
        unsafe {
            std::env::remove_var("TEST_CRON_INVALID");
        }
    }
}
