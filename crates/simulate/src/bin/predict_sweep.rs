//! Predictor accuracy sweep — measures Chronos forecast quality across
//! (history_days, horizon_hours, eval_date, token) combinations.
//!
//! Reads the token universe from `TOKEN_LIST_PATH` (one token per line,
//! produced upstream by a SQL query against `pool_info` to enforce the
//! strategy's ≥ 100 NEAR wnear-side liquidity filter).
//!
//! Output CSV columns (stdout):
//!   eval_date, token, history_days, horizon_h, predicted, actual,
//!   current, pred_return_pct, actual_return_pct, abs_err_pct,
//!   direction_correct
//!
//! Run with `cargo run --release -p simulate --bin predict_sweep > out.csv`.

use anyhow::Result;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use common::api::chronos::ChronosPredictor;
use common::types::TimeRange;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use futures::stream::{self, StreamExt};
use logging::*;
use persistence::token_rate::TokenRate;
use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;

/// 評価対象 token リスト (1 token / 行)。
/// 上流の psql で `pool_info` から ≥ 100 NEAR wnear-side TVL の token を抽出。
const TOKEN_LIST_PATH: &str = "/tmp/predict_sweep_tokens.txt";

/// sweep する履歴日数。
const HISTORY_DAYS: &[i64] = &[7, 14, 21, 30, 40];

/// sweep する予測 horizon (hours)。Chronos は 1 呼び出しで複数 horizon を返すので
/// 最大値 (360h) で予測し、各 horizon を抽出する。
const HORIZON_HOURS: &[usize] = &[24, 72, 168, 360];

/// sweep する評価日。
///
/// 本番 recording は 2026-04-17〜05-19 に 33 日のギャップがあるため、
/// history が連続して取れるブロックは [03-05〜04-16] と [05-20〜06-03]。
/// predict_sweep は history 30-40日の連続性が重要なので、連続データが
/// 潤沢な旧ブロック [03-05〜04-16] を使い、actual 検証に 168h horizon 先
/// (04-16 まで) が取れる 04-04〜04-09 を eval_dates に置く。
const EVAL_DATES: &[&str] = &[
    "2026-03-29",
    "2026-03-31",
    "2026-04-02",
    "2026-04-04",
    "2026-04-06",
    "2026-04-09",
];

/// Chronos モデル並列度。
const MODEL_THREADS: usize = 4;

/// データ抽出時の actual_price 一致トレランス (hours)。
const ACTUAL_LOOKUP_TOLERANCE_HOURS: i64 = 3;

/// 予測 forecast から horizon にマッチする値を取る際のトレランス (hours)。
const FORECAST_MATCH_TOLERANCE_HOURS: i64 = 2;

fn load_token_list() -> Result<Vec<TokenOutAccount>> {
    let raw = fs::read_to_string(TOKEN_LIST_PATH)?;
    let tokens: Vec<TokenOutAccount> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.trim().parse::<near_sdk::AccountId>().ok())
        .map(|aid| TokenAccount::from(aid).to_out())
        .collect();
    Ok(tokens)
}

fn parse_eval_date(s: &str) -> Result<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(s, "%Y-%m-%d")?;
    let nt = NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap());
    Ok(Utc.from_utc_datetime(&nt))
}

fn approx_match<'a, T>(
    iter: impl IntoIterator<Item = &'a (DateTime<Utc>, T)>,
    target: DateTime<Utc>,
    tolerance: TimeDelta,
) -> Option<&'a (DateTime<Utc>, T)> {
    iter.into_iter()
        .filter(|(ts, _)| (*ts - target).abs() <= tolerance)
        .min_by_key(|(ts, _)| (*ts - target).num_seconds().abs())
}

async fn fetch_history(
    token: &TokenOutAccount,
    quote: &TokenInAccount,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, BigDecimal)>> {
    let range = TimeRange {
        start: start.naive_utc(),
        end: end.naive_utc(),
    };
    let map = TokenRate::get_rates_for_multiple_tokens(std::slice::from_ref(token), quote, &range)
        .await?;
    let Some(rates) = map.get(token) else {
        return Ok(Vec::new());
    };
    let spot = TokenRate::to_spot_rates(rates);
    Ok(spot
        .into_iter()
        .map(|(ts, sr)| {
            (
                DateTime::<Utc>::from_naive_utc_and_offset(ts, Utc),
                sr.to_price().as_bigdecimal().clone(),
            )
        })
        .collect())
}

/// 1 (token, history, eval) 単位の予測ジョブを実行し、CSV 行を返す。
/// 並列度を chronos の rayon プールで制御するため、tokio 側でも `Arc<Predictor>` を共有。
async fn run_one_job(
    token: TokenOutAccount,
    quote: TokenInAccount,
    history_days: i64,
    eval_dt: DateTime<Utc>,
    eval_label: &'static str,
    predictor: Arc<ChronosPredictor>,
    max_horizon: usize,
) -> Vec<String> {
    let history_start = eval_dt - TimeDelta::days(history_days);
    let history = match fetch_history(&token, &quote, history_start, eval_dt).await {
        Ok(h) => h,
        Err(_) => return Vec::new(),
    };
    if history.len() < 5 {
        return Vec::new();
    }
    let (last_ts, current) = match history.last() {
        Some((ts, p)) if !p.is_zero() => (*ts, p.clone()),
        _ => return Vec::new(),
    };
    let data: BTreeMap<DateTime<Utc>, BigDecimal> = history.iter().cloned().collect();

    let forecast_until = last_ts + TimeDelta::hours(max_horizon as i64);
    // adaptive detrend は chronos-rs 本体 (regime==Trending でゲート) に組み込まれた
    // ため、predict_price を呼ぶだけで trending token の detrend が自動適用される。
    let response = match predictor.predict_price(data, forecast_until).await {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let forecast: Vec<(DateTime<Utc>, BigDecimal)> = response.forecast.into_iter().collect();

    let mut out = Vec::new();
    for &hh in HORIZON_HOURS {
        let target = last_ts + TimeDelta::hours(hh as i64);
        let predicted = approx_match(
            forecast.iter(),
            target,
            TimeDelta::hours(FORECAST_MATCH_TOLERANCE_HOURS),
        )
        .map(|(_, v)| v.clone());
        let Some(predicted) = predicted else { continue };

        let actual_range = (
            target - TimeDelta::hours(ACTUAL_LOOKUP_TOLERANCE_HOURS),
            target + TimeDelta::hours(ACTUAL_LOOKUP_TOLERANCE_HOURS),
        );
        let actual_history =
            match fetch_history(&token, &quote, actual_range.0, actual_range.1).await {
                Ok(h) => h,
                Err(_) => continue,
            };
        let actual = approx_match(
            actual_history.iter(),
            target,
            TimeDelta::hours(ACTUAL_LOOKUP_TOLERANCE_HOURS),
        )
        .map(|(_, v)| v.clone());
        let Some(actual) = actual else { continue };
        if actual.is_zero() {
            continue;
        }

        let pred_return = ((&predicted - &current) / &current)
            .to_f64()
            .unwrap_or(f64::NAN);
        let actual_return = ((&actual - &current) / &current)
            .to_f64()
            .unwrap_or(f64::NAN);
        let abs_err = ((&predicted - &actual) / &actual)
            .to_f64()
            .unwrap_or(f64::NAN)
            .abs();
        let direction_correct = pred_return.is_finite()
            && actual_return.is_finite()
            && pred_return.signum() == actual_return.signum()
            && pred_return.abs() > 1e-9
            && actual_return.abs() > 1e-9;

        out.push(format!(
            "{},{},{},{},{},{},{},{:.6},{:.6},{:.6},{}",
            eval_label,
            token,
            history_days,
            hh,
            predicted,
            actual,
            current,
            pred_return * 100.0,
            actual_return * 100.0,
            abs_err * 100.0,
            if direction_correct { 1 } else { 0 }
        ));
    }
    out
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "predict_sweep"));

    let eval_dates: Vec<(DateTime<Utc>, &'static str)> = EVAL_DATES
        .iter()
        .map(|s| parse_eval_date(s).map(|dt| (dt, *s)))
        .collect::<Result<Vec<_>>>()?;

    info!(log, "loading token list"; "path" => TOKEN_LIST_PATH);
    let tokens = load_token_list()?;
    info!(log, "tokens to evaluate"; "count" => tokens.len());

    let quote: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN
        .clone()
        .to_in();
    let predictor = Arc::new(ChronosPredictor::new(MODEL_THREADS)?);
    let max_horizon = *HORIZON_HOURS.iter().max().unwrap_or(&360);
    let total = tokens.len() * HISTORY_DAYS.len() * EVAL_DATES.len();

    // CSV header to stdout.
    println!(
        "eval_date,token,history_days,horizon_h,predicted,actual,current,\
         pred_return_pct,actual_return_pct,abs_err_pct,direction_correct"
    );

    // ジョブを (token, history, eval) の Cartesian product として展開。
    let mut jobs: Vec<(TokenOutAccount, i64, DateTime<Utc>, &'static str)> =
        Vec::with_capacity(total);
    for token in &tokens {
        for &hd in HISTORY_DAYS {
            for &(eval_dt, eval_label) in &eval_dates {
                jobs.push((token.clone(), hd, eval_dt, eval_label));
            }
        }
    }

    let done = Arc::new(Mutex::new(0usize));
    let predictor_for_jobs = Arc::clone(&predictor);
    let log_for_jobs = log.clone();

    // 並列度 4 — chronos の MODEL_THREADS と一致させ rayon プールを飽和。
    let mut stream = stream::iter(jobs.into_iter().map(|(token, hd, eval_dt, eval_label)| {
        let predictor = Arc::clone(&predictor_for_jobs);
        let quote = quote.clone();
        let done = Arc::clone(&done);
        let log = log_for_jobs.clone();
        async move {
            let rows = run_one_job(
                token,
                quote,
                hd,
                eval_dt,
                eval_label,
                predictor,
                max_horizon,
            )
            .await;
            let mut d = done.lock().await;
            *d += 1;
            if *d % 200 == 0 {
                info!(log, "progress"; "done" => *d, "total" => total);
            }
            rows
        }
    }))
    .buffer_unordered(4);

    while let Some(rows) = stream.next().await {
        for line in rows {
            println!("{}", line);
        }
    }

    info!(log, "predict_sweep complete"; "total_combos" => total);
    Ok(())
}
