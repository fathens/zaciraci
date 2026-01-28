//! レート統計・レポートモジュール
//!
//! トークンの価格レートを期間ごとに集計し、統計情報を提供する。

use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime};
use num_traits::Zero;
use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use zaciraci_common::types::TokenPrice;

use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};

#[derive(Clone)]
pub struct SameBaseTokenRates {
    pub points: Vec<Point>,
}

#[derive(Clone)]
pub struct Point {
    pub price: TokenPrice,
    pub timestamp: NaiveDateTime,
}

pub struct StatsInPeriod<U> {
    pub timestamp: NaiveDateTime,
    pub period: Duration,

    pub start: U,
    pub end: U,
    pub average: U,
    pub max: U,
    pub min: U,
}
pub struct ListStatsInPeriod<U>(pub Vec<StatsInPeriod<U>>);

impl SameBaseTokenRates {
    pub async fn load(
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
        trace!(log, "start");
        match TokenRate::get_rates_in_time_range(range, base, quote).await {
            Ok(rates) => {
                trace!(log, "loaded rates"; "rates_count" => rates.len());
                let points = rates
                    .iter()
                    .map(|r| Point {
                        price: r.exchange_rate.to_price(),
                        timestamp: r.timestamp,
                    })
                    .collect();
                Ok(SameBaseTokenRates { points })
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
                Err(e)
            }
        }
    }

    pub fn aggregate(&self, period: Duration) -> ListStatsInPeriod<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::aggregate",
            "rates_count" => self.points.len(),
            "period" => format!("{}", period),
        ));
        trace!(log, "start");

        if self.points.is_empty() {
            return ListStatsInPeriod(Vec::new());
        }

        // タイムスタンプの最小値と最大値を取得
        let min_time = self
            .points
            .first()
            .expect("Points vector is not empty")
            .timestamp;
        let max_time = self
            .points
            .last()
            .expect("Points vector is not empty")
            .timestamp;

        // 期間ごとに統計を計算
        let mut stats = Vec::new();
        let mut current_start = min_time;

        while current_start <= max_time {
            let current_end = current_start + period;
            let rates_in_period: Vec<_> = self
                .points
                .iter()
                .skip_while(|rate| rate.timestamp < current_start)
                .take_while(|rate| rate.timestamp < current_end)
                .collect();

            if !rates_in_period.is_empty() {
                let start_price = &rates_in_period
                    .first()
                    .expect("Rates in period is not empty")
                    .price;
                let end_price = &rates_in_period
                    .last()
                    .expect("Rates in period is not empty")
                    .price;
                let sum: TokenPrice = rates_in_period.iter().map(|p| &p.price).sum();
                let count = rates_in_period.len() as i64;
                let average = &sum / count;
                let max = rates_in_period
                    .iter()
                    .map(|p| &p.price)
                    .max()
                    .expect("Rates in period is not empty");
                let min = rates_in_period
                    .iter()
                    .map(|p| &p.price)
                    .min()
                    .expect("Rates in period is not empty");

                stats.push(StatsInPeriod {
                    timestamp: current_start,
                    period,
                    start: start_price.as_bigdecimal().clone(),
                    end: end_price.as_bigdecimal().clone(),
                    average: average.as_bigdecimal().clone(),
                    max: max.as_bigdecimal().clone(),
                    min: min.as_bigdecimal().clone(),
                });
            }

            current_start = current_end;
        }

        trace!(log, "success"; "stats_count" => stats.len());
        ListStatsInPeriod(stats)
    }
}

impl<U> ListStatsInPeriod<U>
where
    U: Clone + Display,
    U: Add<Output = U> + Sub<Output = U> + Mul<Output = U> + Div<Output = U>,
    U: Zero + PartialOrd + From<i64>,
{
    fn format_decimal(value: &U) -> String {
        let s = value.to_string();
        if s.contains('.') {
            // 小数点以下の末尾の0を削除し、最大9桁まで表示
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 2 {
                let integer_part = parts[0];
                let mut decimal_part = parts[1];

                // 小数点以下が全て0の場合は整数表示
                if decimal_part.chars().all(|c| c == '0') {
                    return integer_part.to_string();
                }

                // 末尾の0を削除
                decimal_part = decimal_part.trim_end_matches('0');

                // 小数点以下が9桁を超える場合は9桁までに制限
                if decimal_part.len() > 9 {
                    decimal_part = &decimal_part[..9];
                }

                // 小数点以下が空になった場合は整数のみ返す
                if decimal_part.is_empty() {
                    return integer_part.to_string();
                }

                format!("{}.{}", integer_part, decimal_part)
            } else {
                s
            }
        } else {
            s
        }
    }

    pub fn describes(&self) -> Vec<String> {
        let log = DEFAULT.new(o!(
            "function" => "ListStatsInPeriod::describes",
            "stats_count" => self.0.len(),
        ));
        trace!(log, "start");
        let mut lines = Vec::new();
        let mut prev = None;
        for stat in self.0.iter() {
            let date = stat.timestamp.to_string();
            let changes = prev
                .map(|p: &StatsInPeriod<U>| {
                    let prev = format!(
                        "from the previous {m} minutes",
                        m = stat.period.num_minutes()
                    );
                    let diff = stat.end.clone() - p.end.clone();
                    if diff.is_zero() {
                        return format!(", no change {prev}");
                    }
                    let dw = if diff < U::zero() {
                        "decrease"
                    } else {
                        "increase"
                    };
                    let change = (diff / p.end.clone()) * 100_i64.into();
                    let change_str = Self::format_decimal(&change);
                    format!(", marking a {change_str} % {dw} {prev}")
                })
                .unwrap_or_default();
            let summary = format!(
                "opened at {start}, closed at {end}, with a high of {max}, a low of {min}, and an average of {ave}",
                start = Self::format_decimal(&stat.start),
                end = Self::format_decimal(&stat.end),
                max = Self::format_decimal(&stat.max),
                min = Self::format_decimal(&stat.min),
                ave = Self::format_decimal(&stat.average),
            );
            let line = format!("{date}, {summary}{changes}");
            trace!(log, "added line";
                "line" => &line,
            );
            lines.push(line);
            prev = Some(stat);
        }
        trace!(log, "success";
           "lines_count" => lines.len(),
        );
        lines
    }
}

#[cfg(test)]
mod tests;
