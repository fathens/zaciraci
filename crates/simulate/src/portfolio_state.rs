use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use common::algorithm::types::TradingAction;
use common::types::{ExchangeRate, TokenOutAccount};
use logging::*;
use persistence::token_rate::TokenRate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Abstraction for token rate and decimals lookups.
/// Allows injecting mock implementations for testing.
pub trait RateProvider: Send + Sync {
    fn get_rate(
        &self,
        token: &TokenOutAccount,
        sim_day: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Option<ExchangeRate>> + Send;

    fn get_decimals(&self, token_id: &str) -> impl std::future::Future<Output = u8> + Send;
}

/// Production RateProvider that queries the database via TokenRate
/// and retrieves decimals via trade::make_get_decimals().
pub struct DbRateProvider;

impl RateProvider for DbRateProvider {
    async fn get_rate(
        &self,
        token: &TokenOutAccount,
        sim_day: DateTime<Utc>,
    ) -> Option<ExchangeRate> {
        let wnear_in = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();
        let get_decimals_fn = trade::make_get_decimals();
        get_rate_at_date(token, &wnear_in, sim_day, &get_decimals_fn).await
    }

    async fn get_decimals(&self, token_id: &str) -> u8 {
        let get_decimals_fn = trade::make_get_decimals();
        get_decimals_fn(token_id).await.unwrap_or(24)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub timestamp: DateTime<Utc>,
    pub total_value_near: f64,
    pub holdings: BTreeMap<String, u128>,
    pub cash_balance: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub token: String,
    pub amount: u128,
    pub price_near: f64,
}

pub struct PortfolioState {
    /// wrap.near balance in yocto
    pub cash_balance: u128,
    /// token_id -> amount in smallest units
    pub holdings: BTreeMap<String, u128>,
    /// token_id -> decimals
    pub decimals: HashMap<String, u8>,
    /// daily snapshots
    pub snapshots: Vec<PortfolioSnapshot>,
    /// trade history
    pub trades: Vec<TradeRecord>,
}

impl PortfolioState {
    pub fn new(initial_capital_yocto: u128) -> Self {
        Self {
            cash_balance: initial_capital_yocto,
            holdings: BTreeMap::new(),
            decimals: HashMap::new(),
            snapshots: Vec::new(),
            trades: Vec::new(),
        }
    }

    /// Apply trading actions from the portfolio optimizer
    pub async fn apply_actions(
        &mut self,
        actions: &[TradingAction],
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<()> {
        let log = DEFAULT.new(o!("function" => "apply_actions"));

        for action in actions {
            match action {
                TradingAction::Hold => {
                    // no-op
                }
                TradingAction::Rebalance { target_weights } => {
                    self.apply_rebalance(target_weights, sim_day, rate_provider, &log)
                        .await?;
                }
                TradingAction::AddPosition { token, weight } => {
                    self.apply_add_position(token, *weight, sim_day, rate_provider, &log)
                        .await?;
                }
                TradingAction::ReducePosition { token, weight } => {
                    self.apply_reduce_position(token, *weight, sim_day, rate_provider, &log)
                        .await?;
                }
                TradingAction::Sell { token, target } => {
                    self.apply_sell(token, target, sim_day, rate_provider, &log)
                        .await?;
                }
                TradingAction::Switch { from, to } => {
                    self.apply_switch(from, to, sim_day, rate_provider, &log)
                        .await?;
                }
            }
        }

        Ok(())
    }

    /// Record a daily portfolio snapshot
    pub async fn record_snapshot(
        &mut self,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<()> {
        let total_value = self
            .calculate_total_value_near(sim_day, rate_provider)
            .await?;

        self.snapshots.push(PortfolioSnapshot {
            timestamp: sim_day,
            total_value_near: total_value,
            holdings: self.holdings.clone(),
            cash_balance: self.cash_balance,
        });

        Ok(())
    }

    /// Calculate total portfolio value in NEAR (as f64)
    pub async fn calculate_total_value_near(
        &self,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<f64> {
        let yocto_per_near: f64 = 1e24;

        // Cash portion (wrap.near)
        let mut total = self.cash_balance as f64 / yocto_per_near;

        // Holdings
        for (token_id, amount) in &self.holdings {
            if *amount == 0 {
                continue;
            }

            let token_out: TokenOutAccount = match token_id.parse() {
                Ok(t) => t,
                Err(_) => continue,
            };

            match rate_provider.get_rate(&token_out, sim_day).await {
                Some(rate) => {
                    let decimals = rate_provider.get_decimals(token_id).await;
                    let token_amount = common::types::TokenAmount::from_smallest_units(
                        BigDecimal::from(*amount),
                        decimals,
                    );
                    let near_value = &token_amount / &rate;
                    let near_f64: f64 = near_value
                        .as_bigdecimal()
                        .to_string()
                        .parse()
                        .unwrap_or(0.0);
                    total += near_f64;
                }
                None => {
                    // No rate available, skip this token's value
                }
            }
        }

        Ok(total)
    }

    async fn apply_rebalance(
        &mut self,
        target_weights: &BTreeMap<TokenOutAccount, f64>,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
        log: &slog::Logger,
    ) -> Result<()> {
        let total_value_yocto = self
            .calculate_total_value_yocto(sim_day, rate_provider)
            .await?;
        if total_value_yocto == 0 {
            return Ok(());
        }

        let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

        // First pass: sell excess holdings back to cash
        for (token_out, target_weight) in target_weights {
            let token_str = token_out.to_string();
            if token_str == wnear_str {
                continue;
            }

            let target_value_yocto = (total_value_yocto as f64 * target_weight) as u128;
            let current_value_yocto = self
                .token_value_yocto(&token_str, sim_day, rate_provider)
                .await;

            if current_value_yocto > target_value_yocto {
                // Sell excess
                let excess_yocto = current_value_yocto - target_value_yocto;
                let sell_amount = self
                    .yocto_to_token_amount(&token_str, excess_yocto, sim_day, rate_provider)
                    .await;

                let current = self.holdings.get(&token_str).copied().unwrap_or(0);
                let actual_sell = sell_amount.min(current);
                if actual_sell > 0 {
                    *self.holdings.entry(token_str.clone()).or_insert(0) -= actual_sell;
                    self.cash_balance += excess_yocto;

                    self.trades.push(TradeRecord {
                        timestamp: sim_day,
                        action: "sell".to_string(),
                        token: token_str.clone(),
                        amount: actual_sell,
                        price_near: excess_yocto as f64 / 1e24,
                    });
                    trace!(log, "rebalance sell"; "token" => &token_str, "amount" => actual_sell);
                }
            }
        }

        // Second pass: buy underweight tokens
        for (token_out, target_weight) in target_weights {
            let token_str = token_out.to_string();
            if token_str == wnear_str {
                continue;
            }

            let target_value_yocto = (total_value_yocto as f64 * target_weight) as u128;
            let current_value_yocto = self
                .token_value_yocto(&token_str, sim_day, rate_provider)
                .await;

            if target_value_yocto > current_value_yocto {
                let deficit_yocto = target_value_yocto - current_value_yocto;
                let buy_yocto = deficit_yocto.min(self.cash_balance);
                if buy_yocto == 0 {
                    continue;
                }

                let buy_amount = self
                    .yocto_to_token_amount(&token_str, buy_yocto, sim_day, rate_provider)
                    .await;

                if buy_amount > 0 {
                    self.cash_balance -= buy_yocto;
                    *self.holdings.entry(token_str.clone()).or_insert(0) += buy_amount;

                    self.trades.push(TradeRecord {
                        timestamp: sim_day,
                        action: "buy".to_string(),
                        token: token_str.clone(),
                        amount: buy_amount,
                        price_near: buy_yocto as f64 / 1e24,
                    });
                    trace!(log, "rebalance buy"; "token" => &token_str, "amount" => buy_amount);
                }
            }
        }

        Ok(())
    }

    async fn apply_add_position(
        &mut self,
        token: &TokenOutAccount,
        weight: f64,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
        log: &slog::Logger,
    ) -> Result<()> {
        let token_str = token.to_string();
        let buy_yocto = (self.cash_balance as f64 * weight) as u128;
        if buy_yocto == 0 {
            return Ok(());
        }

        let buy_amount = self
            .yocto_to_token_amount(&token_str, buy_yocto, sim_day, rate_provider)
            .await;

        if buy_amount > 0 {
            self.cash_balance -= buy_yocto;
            *self.holdings.entry(token_str.clone()).or_insert(0) += buy_amount;

            self.trades.push(TradeRecord {
                timestamp: sim_day,
                action: "add_position".to_string(),
                token: token_str.clone(),
                amount: buy_amount,
                price_near: buy_yocto as f64 / 1e24,
            });
            trace!(log, "add position"; "token" => &token_str, "amount" => buy_amount);
        }

        Ok(())
    }

    async fn apply_reduce_position(
        &mut self,
        token: &TokenOutAccount,
        weight: f64,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
        log: &slog::Logger,
    ) -> Result<()> {
        let token_str = token.to_string();
        let current = self.holdings.get(&token_str).copied().unwrap_or(0);
        let sell_amount = (current as f64 * weight) as u128;
        if sell_amount == 0 {
            return Ok(());
        }

        let sell_yocto = self
            .token_amount_to_yocto(&token_str, sell_amount, sim_day, rate_provider)
            .await;

        if sell_yocto > 0 {
            *self.holdings.entry(token_str.clone()).or_insert(0) -= sell_amount;
            self.cash_balance += sell_yocto;

            self.trades.push(TradeRecord {
                timestamp: sim_day,
                action: "reduce_position".to_string(),
                token: token_str.clone(),
                amount: sell_amount,
                price_near: sell_yocto as f64 / 1e24,
            });
            trace!(log, "reduce position"; "token" => &token_str, "amount" => sell_amount);
        }

        Ok(())
    }

    async fn apply_sell(
        &mut self,
        token: &TokenOutAccount,
        target: &TokenOutAccount,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
        log: &slog::Logger,
    ) -> Result<()> {
        let token_str = token.to_string();
        let target_str = target.to_string();
        let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

        // Step 1: sell token -> cash
        let current = self.holdings.get(&token_str).copied().unwrap_or(0);
        if current == 0 {
            return Ok(());
        }

        let sell_yocto = self
            .token_amount_to_yocto(&token_str, current, sim_day, rate_provider)
            .await;
        self.holdings.remove(&token_str);
        self.cash_balance += sell_yocto;

        self.trades.push(TradeRecord {
            timestamp: sim_day,
            action: "sell".to_string(),
            token: token_str.clone(),
            amount: current,
            price_near: sell_yocto as f64 / 1e24,
        });

        // Step 2: cash -> target (if target is not wrap.near)
        if target_str != wnear_str {
            let buy_amount = self
                .yocto_to_token_amount(&target_str, sell_yocto, sim_day, rate_provider)
                .await;
            if buy_amount > 0 {
                self.cash_balance -= sell_yocto;
                *self.holdings.entry(target_str.clone()).or_insert(0) += buy_amount;

                self.trades.push(TradeRecord {
                    timestamp: sim_day,
                    action: "buy".to_string(),
                    token: target_str.clone(),
                    amount: buy_amount,
                    price_near: sell_yocto as f64 / 1e24,
                });
            }
        }

        trace!(log, "sell"; "from" => &token_str, "to" => &target_str);
        Ok(())
    }

    async fn apply_switch(
        &mut self,
        from: &TokenOutAccount,
        to: &TokenOutAccount,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
        log: &slog::Logger,
    ) -> Result<()> {
        let from_str = from.to_string();
        let to_str = to.to_string();

        let current = self.holdings.get(&from_str).copied().unwrap_or(0);
        if current == 0 {
            return Ok(());
        }

        // Convert from -> yocto -> to
        let yocto_value = self
            .token_amount_to_yocto(&from_str, current, sim_day, rate_provider)
            .await;
        self.holdings.remove(&from_str);

        let buy_amount = self
            .yocto_to_token_amount(&to_str, yocto_value, sim_day, rate_provider)
            .await;
        if buy_amount > 0 {
            *self.holdings.entry(to_str.clone()).or_insert(0) += buy_amount;
        }

        self.trades.push(TradeRecord {
            timestamp: sim_day,
            action: "switch".to_string(),
            token: format!("{} -> {}", from_str, to_str),
            amount: current,
            price_near: yocto_value as f64 / 1e24,
        });

        trace!(log, "switch"; "from" => &from_str, "to" => &to_str);
        Ok(())
    }

    /// Calculate total portfolio value in yoctoNEAR
    async fn calculate_total_value_yocto(
        &self,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<u128> {
        let mut total = self.cash_balance;

        for (token_id, amount) in &self.holdings {
            if *amount == 0 {
                continue;
            }
            total += self
                .token_amount_to_yocto(token_id, *amount, sim_day, rate_provider)
                .await;
        }

        Ok(total)
    }

    /// Convert token amount to yoctoNEAR using rates from provider
    async fn token_amount_to_yocto(
        &self,
        token_id: &str,
        amount: u128,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> u128 {
        let token_out: TokenOutAccount = match token_id.parse() {
            Ok(t) => t,
            Err(_) => return 0,
        };

        match rate_provider.get_rate(&token_out, sim_day).await {
            Some(rate) => {
                if rate.is_zero() {
                    return 0;
                }
                let decimals = rate_provider.get_decimals(token_id).await;
                let token_amount = common::types::TokenAmount::from_smallest_units(
                    BigDecimal::from(amount),
                    decimals,
                );
                let near_value = &token_amount / &rate;
                // NearValue -> yoctoNEAR
                let yocto = near_value.to_yocto();
                yocto
                    .as_bigdecimal()
                    .to_string()
                    .parse::<u128>()
                    .unwrap_or(0)
            }
            None => 0,
        }
    }

    /// Convert yoctoNEAR to token amount using rates from provider
    async fn yocto_to_token_amount(
        &self,
        token_id: &str,
        yocto: u128,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> u128 {
        let token_out: TokenOutAccount = match token_id.parse() {
            Ok(t) => t,
            Err(_) => return 0,
        };

        match rate_provider.get_rate(&token_out, sim_day).await {
            Some(rate) => {
                if rate.is_zero() {
                    return 0;
                }
                // yoctoNEAR -> NearValue -> TokenAmount via rate
                let near_value =
                    common::types::YoctoValue::from_yocto(BigDecimal::from(yocto)).to_near();
                let token_amount = &near_value * &rate;
                // TokenAmount -> smallest_units -> u128
                use num_traits::ToPrimitive;
                token_amount.smallest_units().to_u128().unwrap_or(0)
            }
            None => 0,
        }
    }

    /// Get token value in yoctoNEAR
    async fn token_value_yocto(
        &self,
        token_id: &str,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> u128 {
        let amount = self.holdings.get(token_id).copied().unwrap_or(0);
        if amount == 0 {
            return 0;
        }
        self.token_amount_to_yocto(token_id, amount, sim_day, rate_provider)
            .await
    }
}

/// Get the exchange rate closest to (but not after) the given date
async fn get_rate_at_date(
    token_out: &TokenOutAccount,
    wnear_in: &common::types::TokenInAccount,
    sim_day: DateTime<Utc>,
    get_decimals: &persistence::token_rate::GetDecimalsFn,
) -> Option<ExchangeRate> {
    // Use a time range ending at sim_day, look back 24 hours for the latest rate
    let range = common::types::TimeRange {
        start: (sim_day - chrono::Duration::hours(24)).naive_utc(),
        end: sim_day.naive_utc(),
    };

    match TokenRate::get_rates_in_time_range(&range, token_out, wnear_in, get_decimals).await {
        Ok(rates) if !rates.is_empty() => {
            // Return the last (most recent) rate
            Some(rates.last().unwrap().exchange_rate.clone())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
