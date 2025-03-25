use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime};
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::{Arc, Mutex};
use num_traits::Zero;

struct SameBaseTokenRates(Vec<TokenRate>);

pub struct StatsInPeriod<U> {
    pub timestamp: NaiveDateTime,
    pub period: Duration,

    pub start: U,
    pub end: U,
    pub average: U,
    pub max: U,
    pub min: U,
}
struct ListStatsInPeriod<U>(Vec<StatsInPeriod<U>>);

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "trade::start"));

    let now = chrono::Utc::now().naive_utc();
    let range = &TimeRange {
        start: now - Duration::hours(1),
        end: now,
    };
    let period = Duration::minutes(1);

    let quote = get_top_quote_token(range).await?;
    let bases = get_base_tokens(range, &quote).await?;
    let rates_by_base = Arc::new(Mutex::new(HashMap::new()));
    for base in bases.into_iter() {
        match TokenRate::get_rates_in_time_range(range, &base, &quote).await {
            Ok(rates) => {
                let stats = SameBaseTokenRates(rates);
                rates_by_base.lock().unwrap().insert(base, stats);
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
            }
        }
    }
    let stats_by_base: HashMap<TokenOutAccount, _> = rates_by_base
        .lock()
        .unwrap()
        .iter()
        .map(|(base, stats)| (base.clone(), stats.stats(period)))
        .collect();
    let _descs_by_base: HashMap<_, _> = stats_by_base
        .iter()
        .map(|(base, stats)| (base.clone(), stats.describes()))
        .collect();

    info!(log, "success");
    Ok(())
}

async fn get_top_quote_token(range: &TimeRange) -> Result<TokenInAccount> {
    let log = DEFAULT.new(o!("function" => "trade::get_top_quote_token"));

    let quotes = TokenRate::get_quotes_in_time_range(range).await?;
    let (quote, _) = quotes.iter().max_by_key(|(_, c)| *c).unwrap();

    info!(log, "success");
    Ok(quote.clone())
}

async fn get_base_tokens(
    range: &TimeRange,
    quote: &TokenInAccount,
) -> Result<Vec<TokenOutAccount>> {
    let log = DEFAULT.new(o!("function" => "trade::get_base_tokens"));

    let bases = TokenRate::get_bases_in_time_range(range, quote).await?;
    let max_count = bases.iter().max_by_key(|(_, c)| *c).unwrap().1;
    let limit = max_count / 2;
    let tokens = bases
        .iter()
        .filter(|(_, c)| *c > limit)
        .map(|(t, _)| t.clone())
        .collect();

    info!(log, "success");
    Ok(tokens)
}

impl SameBaseTokenRates {
    fn stats(&self, period: Duration) -> ListStatsInPeriod<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "trade::stats_rates",
            "rates_count" => self.0.len(),
            "period" => format!("{}", period),
        ));
        info!(log, "start");

        if self.0.is_empty() {
            return ListStatsInPeriod(Vec::new());
        }

        // タイムスタンプの最小値と最大値を取得
        let min_time = self.0.first().unwrap().timestamp;
        let max_time = self.0.last().unwrap().timestamp;

        // 期間ごとに統計を計算
        let mut stats = Vec::new();
        let mut current_start = min_time;

        while current_start <= max_time {
            let current_end = current_start + period;
            let rates_in_period: Vec<&TokenRate> = self
                .0
                .iter()
                .skip_while(|rate| rate.timestamp < current_start)
                .take_while(|rate| rate.timestamp < current_end)
                .collect();

            if !rates_in_period.is_empty() {
                let start = rates_in_period.first().unwrap().rate.clone();
                let end = rates_in_period.last().unwrap().rate.clone();
                let values: Vec<BigDecimal> =
                    rates_in_period.iter().map(|tr| tr.rate.clone()).collect();
                let sum: BigDecimal = values.iter().sum();
                let count = BigDecimal::from(values.len() as i64);
                let average = &sum / &count;
                let max = values.iter().max().unwrap().clone();
                let min = values.iter().min().unwrap().clone();

                stats.push(StatsInPeriod {
                    timestamp: current_start,
                    period,
                    start,
                    end,
                    average,
                    max,
                    min,
                });
            }

            current_start = current_end;
        }

        info!(log, "success"; "stats_count" => stats.len());
        ListStatsInPeriod(stats)
    }
}

impl<U> ListStatsInPeriod<U>
where
    U: Clone + Display,
    U: Add<Output = U> + Sub<Output = U> + Mul<Output = U> + Div<Output = U>,
    U: Zero + PartialOrd + From<i64>,
{
    pub fn describes(&self) -> Vec<String> {
        let mut lines = Vec::new();
        let mut prev = None;
        for stat in self.0.iter() {
            let date = format!("{}", stat.timestamp);
            let changes = prev
                .map(|p: &StatsInPeriod<U>| {
                    let diff = stat.end.clone() - p.end.clone();
                    if diff.is_zero() {
                        return format!("no change from the previous {:?}", stat.period);
                    }
                    let dw = if diff < U::zero() { "decrease" } else { "increase" };
                    let change = (diff / p.end.clone()) * 100_i64.into();
                    format!(
                        " marking a {:0.2} % {} from the previous {:?}",
                        change,
                        dw,
                        stat.period
                    )
                })
                .unwrap_or_default();
            let summary = format!(
                "opened at {}, closed at {}, with a high of {}, a low of {}, and an average of {}",
                stat.start, stat.end, stat.max, stat.min, stat.average
            );
            lines.push(format!("{}, {}{}", date, summary, changes));
            prev = Some(stat);
        }
        lines
    }
}
