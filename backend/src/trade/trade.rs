use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Duration;

use crate::Result;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use bigdecimal::BigDecimal;

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
    let stats_map = Arc::new(Mutex::new(HashMap::new()));
    for base in bases.into_iter() {
        match TokenRate::get_rates_in_time_range(range, &base, &quote).await {
            Ok(rates) => {
                let stats = stats_rates(&rates, period);
                stats_map.lock().unwrap().insert(base, stats);
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
            }
        }
    }

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

fn stats_rates(rates: &[TokenRate], period: Duration) -> ListStatsInPeriod<BigDecimal> {
    let log = DEFAULT.new(o!("function" => "trade::stats_rates",
        "rates_count" => rates.len(),
        "period" => format!("{}", period),
    ));
    info!(log, "start");

    unimplemented!()
}
