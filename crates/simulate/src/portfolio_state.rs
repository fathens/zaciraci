use anyhow::Result;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
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
    pub realized_pnl_near: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub token: String,
    pub amount: u128,
    pub price_near: f64,
    pub realized_pnl_near: Option<f64>,
}

pub struct PortfolioState {
    /// wrap.near balance in yocto
    pub cash_balance: u128,
    /// token_id -> amount in smallest units
    pub holdings: BTreeMap<String, u128>,
    /// token_id -> decimals (used by mock_client tests)
    #[allow(dead_code)]
    pub decimals: HashMap<String, u8>,
    /// daily snapshots
    pub snapshots: Vec<PortfolioSnapshot>,
    /// trade history
    pub trades: Vec<TradeRecord>,
    /// token_id -> total acquisition cost in yoctoNEAR
    pub cost_basis: BTreeMap<String, u128>,
    /// cumulative realized P&L in yoctoNEAR (signed)
    pub realized_pnl: i128,
    /// token_id -> cumulative realized P&L in yoctoNEAR (signed)
    pub realized_pnl_by_token: BTreeMap<String, i128>,
}

impl PortfolioState {
    pub fn new(initial_capital_yocto: u128) -> Self {
        Self {
            cash_balance: initial_capital_yocto,
            holdings: BTreeMap::new(),
            decimals: HashMap::new(),
            snapshots: Vec::new(),
            trades: Vec::new(),
            cost_basis: BTreeMap::new(),
            realized_pnl: 0,
            realized_pnl_by_token: BTreeMap::new(),
        }
    }

    /// Execute a simulated swap, updating holdings, cash_balance, cost_basis, and realized_pnl.
    ///
    /// Called by SimulationClient::exec_contract when a swap is detected.
    pub fn execute_simulated_swap(
        &mut self,
        from_token: &str,
        from_amount: u128,
        to_token: &str,
        to_amount: u128,
    ) {
        let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();

        // Deduct from source
        if from_token == wnear_str {
            self.cash_balance = self.cash_balance.saturating_sub(from_amount);
        } else {
            let current = self.holdings.get(from_token).copied().unwrap_or(0);
            let actual_deduct = from_amount.min(current);
            if actual_deduct == 0 {
                // Cannot sell a token we don't hold — skip the entire swap
                return;
            }

            *self.holdings.entry(from_token.to_string()).or_insert(0) -= actual_deduct;

            // Determine sell proceeds for P&L calculation.
            // If to_token is WNEAR, to_amount is the NEAR proceeds.
            // For token-to-token swaps, use cost basis as proceeds (0 P&L).
            let sell_proceeds_yocto = if to_token == wnear_str {
                to_amount
            } else {
                self.average_cost_of_sold(from_token, actual_deduct, current)
            };

            self.record_sell_pnl(from_token, actual_deduct, sell_proceeds_yocto);

            // Clean up holdings if position is fully closed
            if self.holdings.get(from_token).copied().unwrap_or(0) == 0 {
                self.holdings.remove(from_token);
            }
        }

        // Add to destination
        if to_token == wnear_str {
            self.cash_balance += to_amount;
        } else {
            *self.holdings.entry(to_token.to_string()).or_insert(0) += to_amount;
            // Track cost basis: the NEAR value of what we spent
            if from_token == wnear_str {
                *self.cost_basis.entry(to_token.to_string()).or_insert(0) += from_amount;
            }
            // TODO: For direct token-to-token swaps (not via WNEAR), the acquired
            // token's cost basis is not tracked. This means selling it later will
            // record the entire proceeds as profit. Currently not an issue because
            // REF Finance swaps are effectively 2-leg (token->WNEAR, WNEAR->token),
            // but should be addressed if direct token-to-token routes are added.
        }
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
            realized_pnl_near: self.realized_pnl as f64 / 1e24,
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

    /// Compute the cost of the sold portion using average cost basis method.
    ///
    /// `total_holding` is the holding amount *before* the sell (including `sell_amount`).
    fn average_cost_of_sold(&self, token_id: &str, sell_amount: u128, total_holding: u128) -> u128 {
        let total_cost = self.cost_basis.get(token_id).copied().unwrap_or(0);
        if sell_amount == total_holding {
            total_cost
        } else if total_holding > 0 {
            total_cost
                .checked_mul(sell_amount)
                .map(|v| v / total_holding)
                .unwrap_or_else(|| (total_cost / total_holding) * sell_amount)
        } else {
            0
        }
    }

    /// Record realized P&L for a sell operation using average cost basis method.
    /// Returns the realized P&L in NEAR (f64).
    fn record_sell_pnl(
        &mut self,
        token_id: &str,
        sell_amount: u128,
        sell_proceeds_yocto: u128,
    ) -> f64 {
        let total_holding = self.holdings.get(token_id).copied().unwrap_or(0) + sell_amount;
        let cost_of_sold = self.average_cost_of_sold(token_id, sell_amount, total_holding);

        let pnl = sell_proceeds_yocto as i128 - cost_of_sold as i128;

        // Update cost basis (subtract the sold portion)
        if let Some(basis) = self.cost_basis.get_mut(token_id) {
            *basis = basis.saturating_sub(cost_of_sold);
        }

        // Accumulate realized P&L
        self.realized_pnl += pnl;
        *self
            .realized_pnl_by_token
            .entry(token_id.to_string())
            .or_insert(0) += pnl;

        // Clean up cost_basis if position is fully closed
        let remaining = self.holdings.get(token_id).copied().unwrap_or(0);
        if remaining == 0 {
            self.cost_basis.remove(token_id);
        }

        pnl as f64 / 1e24
    }

    /// Liquidate all holdings by selling everything back to WNEAR.
    pub async fn liquidate_all(
        &mut self,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<()> {
        let log = DEFAULT.new(o!("function" => "liquidate_all"));

        let tokens: Vec<(String, u128)> = self
            .holdings
            .iter()
            .filter(|&(_, &amount)| amount > 0)
            .map(|(k, &v)| (k.clone(), v))
            .collect();

        for (token_id, amount) in tokens {
            let sell_yocto = self
                .token_amount_to_yocto(&token_id, amount, sim_day, rate_provider)
                .await;

            if sell_yocto == 0 {
                warn!(log, "no rate for liquidation, skipping"; "token" => &token_id);
                continue;
            }

            self.holdings.remove(&token_id);
            self.cash_balance += sell_yocto;

            let pnl_near = self.record_sell_pnl(&token_id, amount, sell_yocto);

            self.trades.push(TradeRecord {
                timestamp: sim_day,
                action: "liquidation".to_string(),
                token: token_id.clone(),
                amount,
                price_near: sell_yocto as f64 / 1e24,
                realized_pnl_near: Some(pnl_near),
            });

            trace!(log, "liquidated"; "token" => &token_id, "amount" => amount, "proceeds" => sell_yocto as f64 / 1e24);
        }

        Ok(())
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
                use num_traits::ToPrimitive;
                yocto.as_bigdecimal().to_u128().unwrap_or(0)
            }
            None => 0,
        }
    }
}

/// Default lookback window for rate queries (24 hours).
const DEFAULT_RATE_LOOKBACK_HOURS: i64 = 24;

/// Get the exchange rate closest to (but not after) the given date.
///
/// Searches within a lookback window ending at `sim_day`. The window size
/// defaults to [`DEFAULT_RATE_LOOKBACK_HOURS`]; use
/// [`get_rate_at_date_with_lookback`] to customise.
pub(crate) async fn get_rate_at_date(
    token_out: &TokenOutAccount,
    wnear_in: &common::types::TokenInAccount,
    sim_day: DateTime<Utc>,
    get_decimals: &persistence::token_rate::GetDecimalsFn,
) -> Option<ExchangeRate> {
    get_rate_at_date_with_lookback(
        token_out,
        wnear_in,
        sim_day,
        get_decimals,
        DEFAULT_RATE_LOOKBACK_HOURS,
    )
    .await
}

/// Like [`get_rate_at_date`], but with a configurable lookback window.
pub(crate) async fn get_rate_at_date_with_lookback(
    token_out: &TokenOutAccount,
    wnear_in: &common::types::TokenInAccount,
    sim_day: DateTime<Utc>,
    get_decimals: &persistence::token_rate::GetDecimalsFn,
    lookback_hours: i64,
) -> Option<ExchangeRate> {
    let range = common::types::TimeRange {
        start: (sim_day - chrono::Duration::hours(lookback_hours)).naive_utc(),
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
