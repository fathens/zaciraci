use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime};
use futures_util::future::join_all;
use num_traits::Zero;
use std::collections::HashMap;
use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
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
    let target = now + Duration::hours(1);

    let _rates_by_base = forcast_rates(range, period, target).await?;

    info!(log, "success");
    Ok(())
}

async fn forcast_rates(
    range: &TimeRange,
    period: Duration,
    target: NaiveDateTime,
) -> Result<HashMap<TokenOutAccount, BigDecimal>> {
    let log = DEFAULT.new(o!("function" => "trade::forcast_rates"));
    info!(log, "start");
    let quote = get_top_quote_token(range).await?;
    let bases = get_base_tokens(range, &quote).await?;
    let rates_by_base = Arc::new(Mutex::new(HashMap::new()));
    let ps = bases.iter().map(|base| async {
        let rates = SameBaseTokenRates::load(&quote, base, range).await?;
        let result = rates.forcast(period, target).await?;
        rates_by_base.lock().unwrap().insert(base.clone(), result);
        Ok::<(), anyhow::Error>(())
    });
    join_all(ps).await;
    info!(log, "success");
    Ok(rates_by_base.lock().unwrap().clone())
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
    async fn load(
        quote: &TokenInAccount,
        base: &TokenOutAccount,
        range: &TimeRange,
    ) -> Result<Self> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::load",
            "base" => base.to_string(),
            "quote" => quote.to_string(),
            "start" => format!("{:?}", range.start),
            "end" => format!("{:?}", range.end),
        ));
        info!(log, "start");
        match TokenRate::get_rates_in_time_range(range, base, quote).await {
            Ok(rates) => {
                info!(log, "loaded rates"; "rates_count" => rates.len());
                Ok(SameBaseTokenRates(rates))
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
                Err(e)
            }
        }
    }

    async fn forcast(&self, period: Duration, target: NaiveDateTime) -> Result<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::forcast",
            "period" => format!("{}", period),
            "target" => format!("{:?}", target),
        ));
        info!(log, "start");

        let stats = self.stats(period);
        let _descs = stats.describes();

        info!(log, "success");
        unimplemented!()
    }

    fn stats(&self, period: Duration) -> ListStatsInPeriod<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::stats",
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
                    let prev = format!("from the previous {} minutes", stat.period.num_minutes());
                    let diff = stat.end.clone() - p.end.clone();
                    if diff.is_zero() {
                        return format!(", no change {}", prev);
                    }
                    let dw = if diff < U::zero() {
                        "decrease"
                    } else {
                        "increase"
                    };
                    let change = (diff / p.end.clone()) * 100_i64.into();
                    format!(", marking a {:0.0} % {} {}", change, dw, prev)
                })
                .unwrap_or_default();
            let summary = format!(
                "opened at {:0.0}, closed at {:0.0}, with a high of {:0.0}, a low of {:0.0}, and an average of {:0.0}",
                stat.start, stat.end, stat.max, stat.min, stat.average
            );
            lines.push(format!("{}, {}{}", date, summary, changes));
            prev = Some(stat);
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_describes() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![]);
        assert!(stats.describes().is_empty());
    }

    #[test]
    fn test_describes_increase() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(101),
                end: BigDecimal::from(100),
                max: BigDecimal::from(102),
                min: BigDecimal::from(90),
                average: BigDecimal::from(95),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(150),
                max: BigDecimal::from(155),
                min: BigDecimal::from(140),
                average: BigDecimal::from(147),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("increase"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 101, closed at 100, with a high of 102, a low of 90, and an average of 95",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 150, with a high of 155, a low of 140, and an average of 147, marking a 50 % increase from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_decrease() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(100),
                max: BigDecimal::from(100),
                min: BigDecimal::from(100),
                average: BigDecimal::from(100),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(50),
                max: BigDecimal::from(50),
                min: BigDecimal::from(50),
                average: BigDecimal::from(50),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("decrease"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 50, with a high of 50, a low of 50, and an average of 50, marking a -50 % decrease from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_no_change() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(100),
                max: BigDecimal::from(100),
                min: BigDecimal::from(100),
                average: BigDecimal::from(100),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(100),
                max: BigDecimal::from(100),
                min: BigDecimal::from(100),
                average: BigDecimal::from(100),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("no change"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100, no change from the previous 1 minutes"
            ]
        );
    }
}
