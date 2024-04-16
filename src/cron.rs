use crate::logging;
use chrono::prelude::Utc;
use cron::Schedule;
use slog::*;
use std::str::FromStr;
use std::time::Duration;
use tokio::spawn;
use tokio::time::sleep;

pub async fn run() {
    let handle = spawn(async {
        let schedule = Schedule::from_str("0 0 * * * *").unwrap();
        let mut next_tick = schedule.upcoming(Utc).next().unwrap();
        loop {
            let now = Utc::now();
            if now >= next_tick {
                info!(logging::DEFAULT, "CRON"; "time" => %now);
                next_tick = schedule.upcoming(Utc).next().unwrap();
            }

            sleep(Duration::from_secs((next_tick - now).num_seconds() as u64)).await;
        }
    });

    handle.await.unwrap();
}
