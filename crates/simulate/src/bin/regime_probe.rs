//! Regime detection probe — measures the Lo-MacKinlay Variance Ratio Test
//! outputs (variance_ratio, z_statistic, p_value, regime) that chronos-rs
//! uses to gate adaptive detrend.
//!
//! Purpose: chronos-rs reported that adaptive detrend over-applies to
//! mean-reverting tokens (stablecoin DirAcc 80%→40%). They need real
//! (z_statistic, variance_ratio) values for stable vs trending tokens to
//! choose a threshold for the proposed hybrid gate
//! (`p < 0.05 && |VR-1| > magnitude`). This bin runs `detect_regime` over
//! the same token universe / history windows as predict_sweep and emits
//! per-(token, history, eval) regime metrics so the threshold can be set
//! from data rather than guessed.
//!
//! Output CSV columns (stdout):
//!   eval_date, token, history_days, n_points, regime, variance_ratio,
//!   z_statistic, p_value, lag
//!
//! Run with `cargo run --release -p simulate --bin regime_probe > out.csv`.

use anyhow::Result;
use bigdecimal::{ToPrimitive, Zero};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc};
use common::types::TimeRange;
use common::types::{TokenAccount, TokenInAccount, TokenOutAccount};
use logging::*;
use persistence::token_rate::TokenRate;

/// 評価対象 token リスト (1 token / 行)。predict_sweep と共通。
const TOKEN_LIST_PATH: &str = "/tmp/predict_sweep_tokens.txt";

/// regime 検出に使う履歴日数。predict_sweep の sweep 軸と揃える。
const HISTORY_DAYS: &[i64] = &[7, 14, 21, 30, 40];

/// 評価日。predict_sweep と同じ旧ブロック (history 連続) を使う。
const EVAL_DATES: &[&str] = &[
    "2026-03-29",
    "2026-03-31",
    "2026-04-02",
    "2026-04-04",
    "2026-04-06",
    "2026-04-09",
];

fn load_token_list() -> Result<Vec<TokenOutAccount>> {
    let raw = std::fs::read_to_string(TOKEN_LIST_PATH)?;
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

/// 履歴 (timestamp, price) を昇順で返す。predict_sweep と同じ経路。
/// slope 計算のため timestamp も保持する。
async fn fetch_series(
    token: &TokenOutAccount,
    quote: &TokenInAccount,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<(NaiveDateTime, f64)>> {
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
                p.as_bigdecimal().to_f64().map(|v| (ts, v))
            }
        })
        .collect::<Vec<(NaiveDateTime, f64)>>())
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "regime_probe"));

    let eval_dates: Vec<(DateTime<Utc>, &'static str)> = EVAL_DATES
        .iter()
        .map(|s| parse_eval_date(s).map(|dt| (dt, *s)))
        .collect::<Result<Vec<_>>>()?;

    let tokens = load_token_list()?;
    info!(log, "regime probe start"; "tokens" => tokens.len());

    let quote: TokenInAccount = blockchain::ref_finance::token_account::WNEAR_TOKEN
        .clone()
        .to_in();
    let analyzer = analyzer::TimeSeriesAnalyzer::new();

    // detrend gate 候補の slope magnitude 評価。
    // slope は「1 サンプル間隔あたりの価格変化」(analyze の x = sample index)。
    // span_ratio = slope × (n-1) / current_price = 履歴期間全体で trend が
    // 説明する相対価格変化。サンプル間隔の仮定を要さず堅牢。chronos-rs 側で
    // この値から detrend gate 閾値 θ を理論ベースに決められる。
    println!(
        "eval_date,token,history_days,n_points,regime,variance_ratio,z_statistic,p_value,lag,\
         slope,current_price,span_ratio"
    );

    for token in &tokens {
        for &hd in HISTORY_DAYS {
            for &(eval_dt, eval_label) in &eval_dates {
                let start = eval_dt - TimeDelta::days(hd);
                let series = match fetch_series(token, &quote, start, eval_dt).await {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if series.is_empty() {
                    continue;
                }
                let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
                let timestamps: Vec<NaiveDateTime> = series.iter().map(|(t, _)| *t).collect();
                let current_price = *values.last().unwrap();

                // detect_regime returns RandomWalk default for < 20 points.
                let info = analyzer.detect_regime(&values);
                // analyze の trend.slope は「1 サンプル間隔あたりの価格変化」。
                // サンプル間隔は ~15min なので、HORIZON_H 時間 = HORIZON_H*4 サンプル先の
                // 想定変化に換算する。slope_horizon_ratio はその相対値。
                let chars = analyzer.analyze(&values, &timestamps);
                let slope = chars.trend.slope;
                let span_ratio = if current_price > 0.0 && values.len() > 1 {
                    slope * (values.len() as f64 - 1.0) / current_price
                } else {
                    0.0
                };

                println!(
                    "{},{},{},{},{:?},{:.6},{:.6},{:.6},{},{:.6e},{:.6e},{:.6}",
                    eval_label,
                    token,
                    hd,
                    values.len(),
                    info.regime,
                    info.variance_ratio,
                    info.z_statistic,
                    info.p_value,
                    info.lag,
                    slope,
                    current_price,
                    span_ratio,
                );
            }
        }
    }

    info!(log, "regime probe complete");
    Ok(())
}
