use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Duration;

use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;

struct SameBaseTokenRates(Vec<TokenRate>);

pub struct StatsInPeriod<U> {
    pub start: U,
    pub end: U,
    pub average: U,
    pub max: U,
    pub min: U,
}
type ListStatsInPeriod<U> = Vec<StatsInPeriod<U>>;

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "trade::start"));

    let now = chrono::Utc::now().naive_utc();
    let range = &TimeRange {
        start: now - chrono::Duration::hours(1),
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
    let _stats_by_base: HashMap<TokenOutAccount, _> = rates_by_base.lock().unwrap().iter().map(|(base, stats)| (base.clone(), stats.stats(period))).collect();

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
            return Vec::new();
        }

        // タイムスタンプの最小値と最大値を取得
        let min_time = self.0.first().unwrap().timestamp;
        let max_time = self.0.last().unwrap().timestamp;

        // 期間ごとに統計を計算
        let mut stats = Vec::new();
        let mut current_start = min_time;
        
        while current_start <= max_time {
            let current_end = current_start + period;
            let rates_in_period: Vec<&TokenRate> = self.0
                .iter()
                .skip_while(|rate| rate.timestamp < current_start)
                .take_while(|rate| rate.timestamp < current_end)
                .collect();

            if !rates_in_period.is_empty() {
                let start = rates_in_period.first().unwrap().rate.clone();
                let end = rates_in_period.last().unwrap().rate.clone();
                let values: Vec<BigDecimal> = rates_in_period.iter().map(|tr| tr.rate.clone()).collect();
                let sum: BigDecimal = values.iter().sum();
                let count = BigDecimal::from(values.len() as i64);
                let average = &sum / &count;
                let max = values.iter().max().unwrap().clone();
                let min = values.iter().min().unwrap().clone();

                stats.push(StatsInPeriod {
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
        stats
    }
}
