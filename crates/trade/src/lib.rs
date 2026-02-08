#![deny(warnings)]

pub mod execution;
pub mod harvest;
pub mod market_data;
pub mod predict;
pub mod prediction_accuracy;
pub mod recorder;
pub mod strategy;
pub mod swap;
pub mod token_cache;

type Result<T> = anyhow::Result<T>;

use bigdecimal::BigDecimal;
use blockchain::jsonrpc;
use blockchain::ref_finance;
use blockchain::ref_finance::token_account::TokenInAccount;
use blockchain::ref_finance::token_account::WNEAR_TOKEN;
use chrono::Utc as TZ;
use common::config;
use common::types::NearAmount;
use common::types::TokenAmount;
use logging::*;
use persistence::token_rate::TokenRate;
use std::future::Future;
use std::sync::Arc;

/// token_rate の backfill 用 decimals 取得コールバックを返す。
///
/// persistence クレートの TokenRate メソッド（get_latest, get_rates_in_time_range 等）に渡す。
/// RPC client を使わず、キャッシュ + RPC のフォールバックで decimals を取得する。
pub fn make_get_decimals()
-> impl Fn(&str) -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::Result<u8>> + Send + '_>>
+ Send
+ Sync {
    |token: &str| {
        let token = token.to_string();
        Box::pin(async move {
            let client = blockchain::jsonrpc::new_client();
            token_cache::get_token_decimals_cached(&client, &token).await
        })
    }
}

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
    cronjob(schedule, strategy::start, "auto_trade").await;
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
                debug!(log, "waiting for next execution";
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
    // config からフィルタ基準を取得し、10% でレート計算（スリッページ最大9%を保証）
    let min_pool = config::get("TRADE_MIN_POOL_LIQUIDITY")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(100);

    let rate_calc_amount = (min_pool / 10).max(1);
    rate_calc_amount.to_string().parse().unwrap()
}

async fn record_rates() -> Result<()> {
    use dex::TokenPairLike;
    use persistence::token_rate::{SwapPath, SwapPoolInfo};

    let log = DEFAULT.new(o!("function" => "record_rates"));

    let quote_token = &get_quote_token();
    let initial_value = get_initial_value();

    trace!(log, "recording rates";
        "quote_token" => %quote_token,
        "initial_value" => %initial_value,
    );

    let client = &jsonrpc::new_client();

    trace!(log, "loading pools");
    let pools = ref_finance::pool_info::read_pools_from_node(client).await?;
    persistence::pool_info::write_to_db(&pools).await?;

    trace!(log, "updating graph");
    let graph = ref_finance::path::graph::TokenGraph::new(Arc::clone(&pools));
    let goals = graph.update_graph(quote_token)?;
    trace!(log, "found targets"; "goals" => %goals.len());
    // NearAmount → YoctoAmount → u128 に変換して list_values_with_path に渡す
    let initial_yocto = initial_value.to_yocto().to_u128();
    let values = graph.list_values_with_path(initial_yocto, quote_token, &goals)?;

    let log = log.new(o!(
        "num_values" => values.len().to_string(),
    ));

    trace!(log, "converting to rates (yocto scale)");

    // 各トークンの decimals を取得（キャッシュ経由、DB 初期化済み）
    let token_ids: Vec<String> = values
        .iter()
        .map(|(base, _, _)| base.to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let token_decimals = token_cache::ensure_decimals_cached(client, &token_ids).await;

    // ExchangeRate を使って TokenRate を構築
    // TokenAmount / NearAmount → ExchangeRate で型安全に rate を計算
    // rate_calc_near: レート計算に使用した NEAR 量を記録
    let rate_calc_near = initial_value.to_i64();
    let rates: Vec<_> = values
        .into_iter()
        .filter_map(|(base, value, path)| {
            let token_str = base.to_string();
            match token_decimals.get(&token_str).copied() {
                Some(decimals) => {
                    let amount =
                        TokenAmount::from_smallest_units(BigDecimal::from(value), decimals);
                    let exchange_rate = &amount / &initial_value;

                    // パス情報を SwapPath に変換
                    let swap_path = SwapPath {
                        pools: path
                            .0
                            .iter()
                            .filter_map(|pair| {
                                let amount_in = pair.amount_in().ok()?;
                                let amount_out = pair.amount_out().ok()?;
                                Some(SwapPoolInfo {
                                    pool_id: pair.pool_id(),
                                    token_in_idx: pair.token_in.as_index().as_u8(),
                                    token_out_idx: pair.token_out.as_index().as_u8(),
                                    amount_in: amount_in.to_string(),
                                    amount_out: amount_out.to_string(),
                                })
                            })
                            .collect(),
                    };

                    Some(TokenRate::new_with_path(
                        base,
                        quote_token.clone(),
                        exchange_rate,
                        rate_calc_near,
                        swap_path,
                    ))
                }
                None => {
                    warn!(log, "skipping token: decimals unknown"; "token" => &token_str);
                    None
                }
            }
        })
        .collect();

    trace!(log, "inserting rates");
    TokenRate::batch_insert(&rates).await?;

    debug!(log, "success");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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

    #[test]
    #[serial]
    fn test_get_initial_value_default() {
        // デフォルト: 100 NEAR → 10% = 10 NEAR
        unsafe {
            std::env::remove_var("TRADE_MIN_POOL_LIQUIDITY");
        }
        common::config::set("TRADE_MIN_POOL_LIQUIDITY", "100");
        let value = get_initial_value();
        assert_eq!(value.to_string(), "10 NEAR");
    }

    #[test]
    #[serial]
    fn test_get_initial_value_custom() {
        // 200 NEAR → 10% = 20 NEAR
        common::config::set("TRADE_MIN_POOL_LIQUIDITY", "200");
        let value = get_initial_value();
        assert_eq!(value.to_string(), "20 NEAR");
    }

    #[test]
    #[serial]
    fn test_get_initial_value_min_1() {
        // 5 NEAR → 10% = 0 → max(1) = 1 NEAR
        common::config::set("TRADE_MIN_POOL_LIQUIDITY", "5");
        let value = get_initial_value();
        assert_eq!(value.to_string(), "1 NEAR");
    }
}
