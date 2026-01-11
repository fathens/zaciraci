pub mod harvest;
pub mod predict;
pub mod recorder;
pub mod stats;
pub mod swap;

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
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::Utc as TZ;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use zaciraci_common::types::ExchangeRate;
use zaciraci_common::types::NearValue;

pub async fn run() {
    tokio::spawn(run_record_rates());
    tokio::spawn(run_trade());
}

async fn run_record_rates() {
    const CRON_CONF: &str = "0 */5 * * * *"; // 5分間隔
    cronjob(CRON_CONF.parse().unwrap(), record_rates, "record_rates").await;
}

async fn run_trade() {
    let log = DEFAULT.new(o!("function" => "run_trade"));
    info!(log, "initializing auto trade cron job");

    // デフォルト: 1日1回（午前0時）、環境変数で設定可能
    let cron_conf =
        config::get("TRADE_CRON_SCHEDULE").unwrap_or_else(|_| "0 0 0 * * *".to_string()); // デフォルト: 毎日午前0時

    info!(log, "cron schedule configured"; "schedule" => &cron_conf);

    match cron_conf.parse() {
        Ok(schedule) => {
            info!(log, "cron schedule parsed successfully");
            cronjob(schedule, stats::start, "auto_trade").await;
        }
        Err(e) => {
            error!(log, "failed to parse cron schedule"; "error" => ?e, "schedule" => &cron_conf);
        }
    }
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
    WNEAR_TOKEN.clone().into()
}

fn get_initial_value() -> NearValue {
    let in_near: u64 = config::get("CRON_RECORD_RATES_INITIAL_VALUE")
        .and_then(|v| Ok(v.parse()?))
        .unwrap_or(100);
    NearValue::new(BigDecimal::from(in_near))
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
    // NearValue → yoctoNEAR (u128) に変換して list_values に渡す
    let initial_yocto = initial_value
        .to_yocto()
        .into_bigdecimal()
        .to_u128()
        .unwrap_or(0);
    let values = graph.list_values(initial_yocto, quote_token, &goals)?;

    let log = log.new(o!(
        "num_values" => values.len().to_string(),
    ));

    // rate_yocto = value / near_amount (e.g. value / 100)
    // これにより後続処理でのスケーリングが不要になる
    let near_amount = initial_value.as_bigdecimal();
    info!(log, "converting to rates (yocto scale)");

    // 各トークンの decimals を取得
    let mut token_decimals: HashMap<String, u8> = HashMap::new();
    for (base, _) in &values {
        let token_str = base.to_string();
        if let std::collections::hash_map::Entry::Vacant(e) =
            token_decimals.entry(token_str.clone())
        {
            let decimals = stats::get_token_decimals(client, &token_str).await;
            e.insert(decimals);
        }
    }

    // ExchangeRate を使って TokenRate を構築
    let rates: Vec<_> = values
        .into_iter()
        .map(|(base, value)| {
            let token_str = base.to_string();
            let decimals = token_decimals.get(&token_str).copied().unwrap_or(24);
            let rate_yocto = BigDecimal::from(value) / near_amount;
            let exchange_rate = ExchangeRate::from_raw_rate(rate_yocto, decimals);
            TokenRate::new(base, quote_token.clone(), exchange_rate)
        })
        .collect();

    info!(log, "inserting rates");
    TokenRate::batch_insert(&rates).await?;

    info!(log, "success");
    Ok(())
}
