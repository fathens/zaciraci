//! Dump per-token price-history JSON fixtures for the chronos-rs
//! `top_decile_decomposition` diagnostic.
//!
//! For each (token, eval_date) the full window
//! `[eval_date - HISTORY_DAYS, eval_date + HORIZON_HOURS]` is written as a
//! single `{description, data: [{timestamp, price}]}` file. The chronos-rs
//! diagnostic (`crates/predictor/tests/real_data_over_damping.rs`) splits the
//! trailing `horizon` window off as the held-out actual internally, so the
//! file must contain both the training history and the horizon-ahead actual.
//!
//! Output goes to the directory given as the first CLI arg (created if
//! missing). Files are named `{token}_{eval}.json` (`/` and `.` in the token
//! id are replaced so the filename stays flat).
//!
//! Run with:
//!   cargo run --release -p simulate --bin dump_fixtures -- <out_dir>

use anyhow::Result;
use bigdecimal::Zero;
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use common::types::TimeRange;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use logging::*;
use persistence::token_rate::TokenRate;
use std::fs;

/// 評価対象 token リスト。predict_sweep と共通。
const TOKEN_LIST_PATH: &str = "/tmp/predict_sweep_tokens.txt";

/// fixture の history 長 (日)。test 側は train>=50 点を要求するので余裕を持たせる。
const HISTORY_DAYS: i64 = 40;

/// horizon (hours)。本番設定 168h。test の DIAG_HORIZON_SECS と一致させること。
const HORIZON_HOURS: i64 = 168;

/// eval_date。predict_sweep の旧ブロックから、168h actual が 04-16 までに収まる日を選ぶ。
const EVAL_DATES: &[&str] = &["2026-03-29", "2026-04-02", "2026-04-06", "2026-04-09"];

fn load_token_list() -> Result<Vec<TokenOutAccount>> {
    let raw = fs::read_to_string(TOKEN_LIST_PATH)?;
    Ok(raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.trim().parse::<near_sdk::AccountId>().ok())
        .map(|aid| TokenAccount::from(aid).to_out())
        .collect())
}

fn parse_eval_date(s: &str) -> Result<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    Ok(Utc.from_utc_datetime(&NaiveDateTime::new(
        date,
        NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
    )))
}

async fn fetch_series(
    token: &TokenOutAccount,
    quote: &TokenInAccount,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(NaiveDateTime, String)>> {
    let range = TimeRange {
        start: start.naive_utc(),
        end: end.naive_utc(),
    };
    let map = TokenRate::get_rates_for_multiple_tokens(std::slice::from_ref(token), quote, &range)
        .await?;
    let Some(rates) = map.get(token) else {
        return Ok(Vec::new());
    };
    Ok(TokenRate::to_spot_rates(rates)
        .into_iter()
        .filter_map(|(ts, sr)| {
            let p = sr.to_price();
            if p.as_bigdecimal().is_zero() {
                None
            } else {
                Some((ts, p.as_bigdecimal().to_string()))
            }
        })
        .collect())
}

fn sanitize(token: &str) -> String {
    token.replace(['/', '.'], "_")
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "dump_fixtures"));

    let out_dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/chronos_fixtures".to_string());
    fs::create_dir_all(&out_dir)?;

    let tokens = load_token_list()?;
    let eval_dates: Vec<(DateTime<Utc>, &'static str)> = EVAL_DATES
        .iter()
        .map(|s| parse_eval_date(s).map(|dt| (dt, *s)))
        .collect::<Result<Vec<_>>>()?;

    let quote: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN
        .clone()
        .to_in();

    info!(log, "dump start"; "tokens" => tokens.len(), "out" => &out_dir);

    let mut written = 0usize;
    for token in &tokens {
        for &(eval_dt, eval_label) in &eval_dates {
            let start = eval_dt - TimeDelta::days(HISTORY_DAYS);
            let end = eval_dt + TimeDelta::hours(HORIZON_HOURS);
            let series = match fetch_series(token, &quote, start, end).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            // test 側は train>=50 点 + actual 非空を要求。余裕を見て 60 点で足切り。
            if series.len() < 60 {
                continue;
            }
            let points: Vec<String> = series
                .iter()
                .map(|(ts, price)| {
                    format!(
                        "    {{\"timestamp\": \"{}\", \"price\": \"{}\"}}",
                        ts.format("%Y-%m-%dT%H:%M:%S%.f"),
                        price
                    )
                })
                .collect();
            let json = format!(
                "{{\n  \"description\": \"{}@{}\",\n  \"data\": [\n{}\n  ]\n}}\n",
                token,
                eval_label,
                points.join(",\n")
            );
            let fname = format!(
                "{}/{}_{}.json",
                out_dir,
                sanitize(&token.to_string()),
                eval_label
            );
            fs::write(&fname, json)?;
            written += 1;
        }
    }

    info!(log, "dump complete"; "files" => written);
    Ok(())
}
