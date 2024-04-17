use crate::logging;
use chrono::Utc as TZ;
use slog::*;

const CRON: &str = "0 0 * * * *";

pub async fn run() {
    let schedule: cron::Schedule = CRON.parse().unwrap();
    for next in schedule.upcoming(TZ) {
        if let Ok(wait) = (next - TZ::now()).to_std() {
            tokio::time::sleep(wait).await;
            job().await;
        }
    }
}

async fn job() {
    info!(logging::DEFAULT, "CRON");
}
