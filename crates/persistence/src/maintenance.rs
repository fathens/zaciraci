use crate::connection_pool;
use common::config::ConfigAccess;
use logging::*;

use crate::Result;

/// REINDEX CONCURRENTLY の対象テーブル
const REINDEX_TARGETS: &[&str] = &["pool_info", "token_rates"];

/// DB メンテナンスの定期実行エントリポイント
pub async fn run(cfg: impl ConfigAccess + 'static) {
    let log = DEFAULT.new(o!("function" => "maintenance::run"));
    info!(log, "starting db maintenance cron job");

    let cron_conf = cfg.db_maintenance_cron_schedule();
    let schedule: cron::Schedule = match cron_conf.parse() {
        Ok(s) => {
            info!(log, "db maintenance schedule configured"; "schedule" => &cron_conf);
            s
        }
        Err(e) => {
            let default = "0 0 4 * * 0";
            error!(log, "failed to parse db maintenance schedule, using default";
                   "error" => ?e, "schedule" => &cron_conf, "default" => default);
            default
                .parse()
                .expect("hardcoded default cron schedule must be valid")
        }
    };

    for next in schedule.upcoming(chrono::Utc) {
        let now = chrono::Utc::now();
        if next <= now {
            continue;
        }

        let wait = match (next - now).to_std() {
            Ok(d) => d,
            Err(_) => continue,
        };

        debug!(log, "waiting for next maintenance";
            "next_time" => %next,
            "wait_seconds" => wait.as_secs()
        );

        // 1分間隔でスリープ（長時間 sleep を避ける）
        loop {
            let now = chrono::Utc::now();
            if now >= next {
                break;
            }
            let remaining = match (next - now).to_std() {
                Ok(d) => d,
                Err(_) => break,
            };
            let max_sleep = cfg.cron_max_sleep_seconds();
            let sleep_duration = remaining.min(std::time::Duration::from_secs(max_sleep));
            tokio::time::sleep(sleep_duration).await;
        }

        // 実行前に DB 設定をリロード
        let instance_id = &common::config::startup::get().instance_id;
        crate::config_store::reload_to_config(instance_id)
            .await
            .ok();

        info!(log, "executing db maintenance");

        for table in REINDEX_TARGETS {
            match reindex_table(table).await {
                Ok(()) => info!(log, "reindex completed"; "table" => *table),
                Err(e) => error!(log, "reindex failed"; "table" => *table, "error" => %e),
            }
        }
    }
}

/// 指定テーブルに REINDEX CONCURRENTLY を実行
async fn reindex_table(table_name: &str) -> Result<()> {
    use diesel::RunQueryDsl;

    // SQL injection 防止: ホワイトリスト検証
    if !REINDEX_TARGETS.contains(&table_name) {
        return Err(anyhow::anyhow!(
            "table '{}' is not in the allowed reindex targets",
            table_name
        ));
    }

    let log = DEFAULT.new(o!(
        "function" => "maintenance::reindex_table",
        "table" => table_name.to_owned(),
    ));
    info!(log, "starting reindex");

    // format! で SQL を組み立てるが、上のホワイトリスト検証で安全
    let sql = format!("REINDEX (CONCURRENTLY) TABLE {table_name}");
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| diesel::sql_query(&sql).execute(conn))
        .await
        .map_err(|e| anyhow::anyhow!("database interaction error: {:?}", e))??;

    info!(log, "reindex completed");
    Ok(())
}
