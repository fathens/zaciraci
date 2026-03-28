#![deny(warnings)]

pub mod execution;
pub mod harvest;
pub mod market_data;
pub mod predict;
pub mod prediction_accuracy;
pub mod recorder;
pub mod slippage;
pub mod snapshot;
pub mod strategy;
pub mod swap;
pub mod token_cache;
pub mod valuation;

type Result<T> = anyhow::Result<T>;

use bigdecimal::BigDecimal;
use blockchain::jsonrpc;
use blockchain::ref_finance;
use blockchain::ref_finance::token_account::WNEAR_TOKEN;
use chrono::Utc as TZ;
use common::config::{ConfigAccess, ConfigResolver};
use common::types::NearAmount;
use common::types::TokenAmount;
use common::types::TokenInAccount;
use common::types::TokenOutAccount;
use logging::*;
use persistence::token_rate::TokenRate;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

pub async fn run(cfg: ConfigResolver) {
    // DB からトークン decimals キャッシュを初期化
    if let Err(e) = token_cache::load_from_db().await {
        let log = DEFAULT.new(o!("function" => "run"));
        error!(log, "failed to load token decimals cache from DB"; "error" => ?e);
    }

    tokio::spawn(run_record_rates(cfg));
    tokio::spawn(run_trade(cfg));
}

/// cron スケジュール文字列をパースし、失敗時は default にフォールバック
fn parse_cron_schedule(cron_expr: &str, default: &str) -> cron::Schedule {
    let log = DEFAULT.new(o!("function" => "parse_cron_schedule"));

    match cron_expr.parse() {
        Ok(s) => {
            info!(log, "cron schedule configured"; "schedule" => cron_expr);
            s
        }
        Err(e) => {
            error!(log, "failed to parse cron schedule, using default";
                   "error" => ?e, "schedule" => cron_expr, "default" => default);
            default
                .parse()
                .expect("hardcoded default cron schedule must be valid")
        }
    }
}

async fn run_record_rates(cfg: ConfigResolver) {
    const DEFAULT_CRON: &str = "0 */15 * * * *";
    let schedule = parse_cron_schedule(&cfg.record_rates_cron_schedule(), DEFAULT_CRON);
    cronjob(schedule, || record_rates(&cfg), "record_rates", &cfg).await;
}

async fn run_trade(cfg: ConfigResolver) {
    const DEFAULT_CRON: &str = "0 0 0 * * *";
    let log = DEFAULT.new(o!("function" => "run_trade"));
    info!(log, "initializing auto trade cron job");

    let schedule = parse_cron_schedule(&cfg.trade_cron_schedule(), DEFAULT_CRON);
    cronjob(
        schedule,
        || async {
            // 予測フェーズ（失敗 → 今回のサイクルをスキップ）
            if let Err(e) = run_predictions(&cfg).await {
                let log = DEFAULT.new(o!("function" => "run_trade"));
                error!(log, "prediction phase failed, skipping trade cycle"; "error" => %e);
                return Ok(());
            }

            let client = blockchain::jsonrpc::new_client();
            let wallet = blockchain::wallet::new_wallet();
            strategy::start(&client, &wallet, chrono::Utc::now(), &cfg).await
        },
        "auto_trade",
        &cfg,
    )
    .await;
}

/// 全対象トークンの価格予測を実行して prediction_records に保存する（本番 cron 用）
async fn run_predictions(cfg: &impl ConfigAccess) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "run_predictions"));

    // 1. 過去予測の評価（ハウスキーピング）
    match prediction_accuracy::evaluate_pending_predictions(cfg).await {
        Ok(count) => {
            if count > 0 {
                info!(log, "evaluated pending predictions"; "count" => count);
            }
        }
        Err(e) => {
            warn!(log, "prediction evaluation failed, continuing"; "error" => %e);
        }
    }

    // 2. 予測サイクル実行
    let count = run_prediction_cycle(chrono::Utc::now(), cfg).await?;
    info!(log, "predictions recorded"; "count" => count);
    Ok(())
}

/// 指定日時を起点に、対象トークンの予測を実行して prediction_records に保存する。
///
/// 内部で `Utc::now()` は使用しない。渡された `as_of` のみを時刻の基準とする。
///
/// 戻り値: 保存した予測の件数
pub async fn run_prediction_cycle(
    as_of: chrono::DateTime<chrono::Utc>,
    cfg: &impl ConfigAccess,
) -> Result<usize> {
    let log = DEFAULT.new(o!("function" => "run_prediction_cycle"));

    // 1. 全対象トークン取得（ボラティリティ＋流動性フィルタ）
    let prediction_service = predict::PredictionService::new(cfg)?;
    let target_tokens =
        strategy::select_prediction_target_tokens(&prediction_service, as_of, cfg).await?;

    info!(log, "prediction targets selected"; "count" => target_tokens.len());

    let quote_token = get_quote_token();
    let price_history_days = i64::from(cfg.trade_price_history_days());
    let token_out_list: Vec<TokenOutAccount> =
        target_tokens.into_iter().map(|t| t.into()).collect();

    // 2. チャンクごとに予測実行（メモリピーク抑制）
    let chunk_size = (cfg.trade_prediction_chunk_size() as usize).max(1);
    let mut prediction_entries: BTreeMap<
        TokenOutAccount,
        (common::types::TokenPrice, chrono::NaiveDateTime),
    > = BTreeMap::new();
    let mut empty_predictions = 0u32;

    for (chunk_idx, chunk) in token_out_list.chunks(chunk_size).enumerate() {
        info!(log, "processing prediction chunk";
            "chunk" => chunk_idx,
            "size" => chunk.len(),
            "total_tokens" => token_out_list.len()
        );

        let predictions = match prediction_service
            .predict_multiple_tokens(
                chunk,
                &quote_token,
                price_history_days,
                prediction_accuracy::PREDICTION_HORIZON_HOURS,
                as_of,
                cfg,
            )
            .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!(log, "prediction chunk failed, skipping";
                    "chunk" => chunk_idx, "error" => %e);
                continue;
            }
        };

        for (token, result) in predictions {
            match result.prediction_at_horizon(prediction_accuracy::PREDICTION_HORIZON_HOURS) {
                Some(p) => {
                    prediction_entries.insert(
                        token,
                        (p.price.clone(), result.data_cutoff_time.naive_utc()),
                    );
                }
                None => empty_predictions = empty_predictions.saturating_add(1),
            }
        }
        // predictions HashMap はここで drop — チャンクの価格履歴メモリを解放
    }

    if empty_predictions > 0 {
        warn!(log, "tokens with empty prediction results"; "count" => empty_predictions);
    }

    if prediction_entries.is_empty() && !token_out_list.is_empty() {
        return Err(anyhow::anyhow!(
            "all prediction chunks failed: 0/{} tokens produced predictions",
            token_out_list.len()
        ));
    }

    // 3. 予測価格を DB に保存
    prediction_accuracy::record_predictions(&prediction_entries, &quote_token).await?;

    Ok(prediction_entries.len())
}

async fn cronjob<F, Fut>(schedule: cron::Schedule, func: F, name: &str, cfg: &impl ConfigAccess)
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

                    // 最大sleep秒数（設定可能、デフォルト60秒）
                    let max_sleep = cfg.cron_max_sleep_seconds();
                    let sleep_duration = remaining.min(std::time::Duration::from_secs(max_sleep));

                    // 長時間待機の場合は定期的にログを出力
                    let log_threshold = cfg.cron_log_threshold_seconds();
                    if remaining.as_secs() > log_threshold {
                        debug!(log, "still waiting for next execution";
                            "remaining_seconds" => remaining.as_secs(),
                            "next_time" => %next
                        );
                    }

                    tokio::time::sleep(sleep_duration).await;
                }

                // タスク実行前に DB から設定をリロード
                let instance_id = &common::config::startup::get().instance_id;
                persistence::config_store::reload_to_config(instance_id)
                    .await
                    .ok();

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

fn get_initial_value(cfg: &impl ConfigAccess) -> NearAmount {
    // config からフィルタ基準を取得し、10% でレート計算（スリッページ最大9%を保証）
    let min_pool = cfg.trade_min_pool_liquidity();

    let rate_calc_amount = (min_pool / 10).max(1);
    rate_calc_amount.to_string().parse().unwrap()
}

async fn record_rates(cfg: &impl ConfigAccess) -> Result<()> {
    use dex::TokenPairLike;
    use persistence::token_rate::{SwapPath, SwapPoolInfo};

    let log = DEFAULT.new(o!("function" => "record_rates"));

    let quote_token = &get_quote_token();
    let initial_value = get_initial_value(cfg);

    trace!(log, "recording rates";
        "quote_token" => %quote_token,
        "initial_value" => %initial_value,
    );

    let client = &jsonrpc::new_client();

    trace!(log, "loading pools");
    let pools = ref_finance::pool_info::read_pools_from_node(client).await?;
    persistence::pool_info::write_to_db(&pools, cfg).await?;

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
    let token_ids: Vec<common::types::TokenAccount> = values
        .iter()
        .map(|(base, _, _)| base.inner().clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    token_cache::cleanup_stale_failures(&token_ids).await;
    let token_decimals = token_cache::ensure_decimals_cached(client, &token_ids, cfg).await;

    // ExchangeRate を使って TokenRate を構築
    // TokenAmount / NearAmount → ExchangeRate で型安全に rate を計算
    // rate_calc_near: レート計算に使用した NEAR 量を記録
    let rate_calc_near = initial_value.to_i64();
    let rates: Vec<_> = values
        .into_iter()
        .filter_map(|(base, value, path)| {
            match token_decimals.get(base.inner()).copied() {
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
                                    amount_in: amount_in.into(),
                                    amount_out: amount_out.into(),
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
                    debug!(log, "skipping token: decimals not available"; "token" => %base);
                    None
                }
            }
        })
        .collect();

    trace!(log, "inserting rates");
    TokenRate::batch_insert(&rates, cfg).await?;

    debug!(log, "success");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_parse_cron_schedule_valid() {
        let schedule = parse_cron_schedule("0 */15 * * * *", "0 0 0 * * *");
        let mut upcoming = schedule.upcoming(TZ);
        let first = upcoming.next().unwrap();
        let second = upcoming.next().unwrap();
        assert_eq!((second - first).num_minutes(), 15);
    }

    #[test]
    fn test_parse_cron_schedule_fallback_on_invalid() {
        let schedule = parse_cron_schedule("invalid cron", "0 */15 * * * *");
        let mut upcoming = schedule.upcoming(TZ);
        let first = upcoming.next().unwrap();
        let second = upcoming.next().unwrap();
        assert_eq!((second - first).num_minutes(), 15); // デフォルトにフォールバック
    }

    #[test]
    #[serial]
    fn test_get_initial_value_default() {
        // デフォルト: 100 NEAR → 10% = 10 NEAR
        let _env_guard = common::config::store::EnvGuard::remove("TRADE_MIN_POOL_LIQUIDITY");
        let _guard = common::config::store::ConfigGuard::new("TRADE_MIN_POOL_LIQUIDITY", "100");
        let cfg = ConfigResolver;
        let value = get_initial_value(&cfg);
        assert_eq!(value.to_string(), "10 NEAR");
    }

    #[test]
    #[serial]
    fn test_get_initial_value_custom() {
        // 200 NEAR → 10% = 20 NEAR
        let _guard = common::config::store::ConfigGuard::new("TRADE_MIN_POOL_LIQUIDITY", "200");
        let cfg = ConfigResolver;
        let value = get_initial_value(&cfg);
        assert_eq!(value.to_string(), "20 NEAR");
    }

    #[test]
    #[serial]
    fn test_get_initial_value_min_1() {
        // 5 NEAR → 10% = 0 → max(1) = 1 NEAR
        let _guard = common::config::store::ConfigGuard::new("TRADE_MIN_POOL_LIQUIDITY", "5");
        let cfg = ConfigResolver;
        let value = get_initial_value(&cfg);
        assert_eq!(value.to_string(), "1 NEAR");
    }
}
