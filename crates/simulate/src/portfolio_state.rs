use anyhow::Result;
use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::{DateTime, Utc};
use common::types::{ExchangeRate, TokenAccount, TokenAmount, TokenOutAccount, YoctoValue};
use logging::*;
use persistence::token_rate::TokenRate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Convert BigDecimal to u128, logging a warning and returning 0 if the value is out of range.
pub(crate) fn to_u128_or_warn(value: &BigDecimal, context: &str) -> u128 {
    value.to_u128().unwrap_or_else(|| {
        let log = DEFAULT.new(o!("function" => "to_u128_or_warn"));
        warn!(log, "BigDecimal value exceeds u128 range, defaulting to 0";
            "context" => context, "value" => %value);
        0
    })
}

/// Abstraction for token rate lookups.
/// Allows injecting mock implementations for testing.
pub trait RateProvider: Send + Sync {
    fn get_rate(
        &self,
        token: &TokenOutAccount,
        sim_day: DateTime<Utc>,
    ) -> impl std::future::Future<Output = Option<ExchangeRate>> + Send;
}

/// Production RateProvider that queries the database via TokenRate.
pub struct DbRateProvider;

impl RateProvider for DbRateProvider {
    async fn get_rate(
        &self,
        token: &TokenOutAccount,
        sim_day: DateTime<Utc>,
    ) -> Option<ExchangeRate> {
        let wnear_in = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();
        get_rate_at_date(token, &wnear_in, sim_day).await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSnapshot {
    pub timestamp: DateTime<Utc>,
    pub total_value_near: f64,
    pub holdings: BTreeMap<TokenAccount, TokenAmount>,
    pub cash_balance: YoctoValue,
    pub realized_pnl_near: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub token: TokenAccount,
    pub amount: TokenAmount,
    pub price_near: f64,
    pub realized_pnl_near: Option<f64>,
}

/// Default token decimals for NEAR ecosystem tokens.
/// Most native tokens use 24 decimals; used when the actual value is unknown.
pub(crate) const DEFAULT_DECIMALS: u8 = 24;

pub struct PortfolioState {
    /// wrap.near balance in yoctoNEAR
    pub cash_balance: YoctoValue,
    /// token -> amount (with decimals)
    pub holdings: BTreeMap<TokenAccount, TokenAmount>,
    /// daily snapshots
    pub snapshots: Vec<PortfolioSnapshot>,
    /// trade history
    pub trades: Vec<TradeRecord>,
    /// token -> total acquisition cost in yoctoNEAR
    pub cost_basis: BTreeMap<TokenAccount, YoctoValue>,
    /// cumulative realized P&L in yoctoNEAR (signed)
    pub realized_pnl: i128,
    /// token -> cumulative realized P&L in yoctoNEAR (signed)
    pub realized_pnl_by_token: BTreeMap<TokenAccount, i128>,
}

impl PortfolioState {
    pub fn new(initial_capital: YoctoValue) -> Self {
        Self {
            cash_balance: initial_capital,
            holdings: BTreeMap::new(),
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
        from_token: &TokenAccount,
        from_amount: u128,
        to_token: &TokenAccount,
        to_amount: u128,
    ) {
        let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;

        // Determine actual deduction and proportionally scale to_amount.
        // If from_amount exceeds available balance, we clamp and scale output
        // to maintain consistent input/output ratio.
        let (actual_from, actual_to) = if from_token == wnear {
            let available = to_u128_or_warn(self.cash_balance.as_bigdecimal(), "cash_balance");
            let actual = from_amount.min(available);
            if actual == 0 {
                return;
            }
            let scaled_to = Self::scale_output(to_amount, actual, from_amount);
            (actual, scaled_to)
        } else {
            let current = self
                .holdings
                .get(from_token)
                .map(|a| to_u128_or_warn(a.smallest_units(), "holdings"))
                .unwrap_or(0);
            let actual = from_amount.min(current);
            if actual == 0 {
                return;
            }
            let scaled_to = Self::scale_output(to_amount, actual, from_amount);
            (actual, scaled_to)
        };

        let from_yocto = YoctoValue::from_yocto(BigDecimal::from(actual_from));
        let to_yocto = YoctoValue::from_yocto(BigDecimal::from(actual_to));

        // Deduct from source
        if from_token == wnear {
            self.cash_balance = self.cash_balance.saturating_sub(&from_yocto);
        } else {
            let current = self
                .holdings
                .get(from_token)
                .map(|a| to_u128_or_warn(a.smallest_units(), "holdings"))
                .unwrap_or(0);

            // Subtract from holdings
            if let Some(holding) = self.holdings.get_mut(from_token) {
                let new_units = holding.smallest_units() - BigDecimal::from(actual_from);
                *holding = TokenAmount::from_smallest_units(new_units, holding.decimals());
            }

            // Determine sell proceeds for P&L calculation.
            let sell_proceeds_yocto = if to_token == wnear {
                actual_to
            } else {
                self.average_cost_of_sold(from_token, actual_from, current)
            };

            self.record_sell_pnl(from_token, actual_from, sell_proceeds_yocto);

            // Clean up holdings if position is fully closed
            if self
                .holdings
                .get(from_token)
                .map(|a| a.is_zero())
                .unwrap_or(true)
            {
                self.holdings.remove(from_token);
            }
        }

        // Add to destination
        if to_token == wnear {
            self.cash_balance = self.cash_balance.clone() + to_yocto;
        } else {
            let entry = self.holdings.entry(to_token.clone()).or_insert_with(|| {
                // Use decimals from existing holdings or default to 24
                TokenAmount::zero(DEFAULT_DECIMALS)
            });
            let new_units = entry.smallest_units() + BigDecimal::from(actual_to);
            *entry = TokenAmount::from_smallest_units(new_units, entry.decimals());

            // Track cost basis: the NEAR value of what we spent
            if from_token == wnear {
                let basis = self
                    .cost_basis
                    .entry(to_token.clone())
                    .or_insert_with(YoctoValue::zero);
                *basis = basis.clone() + from_yocto;
            }
            // TODO: For direct token-to-token swaps (not via WNEAR), the acquired
            // token's cost basis is not tracked. This means selling it later will
            // record the entire proceeds as profit. Currently not an issue because
            // REF Finance swaps are effectively 2-leg (token->WNEAR, WNEAR->token),
            // but should be addressed if direct token-to-token routes are added.
        }
    }

    /// Scale output amount proportionally when actual input is less than requested.
    /// Uses BigDecimal for precision: `to_amount * actual / requested`.
    fn scale_output(to_amount: u128, actual: u128, requested: u128) -> u128 {
        if actual == requested {
            return to_amount;
        }
        let to_bd = BigDecimal::from(to_amount);
        let actual_bd = BigDecimal::from(actual);
        let requested_bd = BigDecimal::from(requested);
        let scaled = (to_bd * actual_bd) / requested_bd;
        to_u128_or_warn(&scaled, "scale_output")
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
            cash_balance: self.cash_balance.clone(),
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
        let mut total = self.cash_balance.as_bigdecimal().to_f64().unwrap_or(0.0) / yocto_per_near;

        // Holdings
        for (token_account, token_amount) in &self.holdings {
            if token_amount.is_zero() {
                continue;
            }

            let token_out: TokenOutAccount = token_account.to_out();

            match rate_provider.get_rate(&token_out, sim_day).await {
                Some(rate) => {
                    let near_value = token_amount / &rate;
                    let near_f64: f64 = near_value.as_bigdecimal().to_f64().unwrap_or(0.0);
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
    fn average_cost_of_sold(
        &self,
        token: &TokenAccount,
        sell_amount: u128,
        total_holding: u128,
    ) -> u128 {
        let total_cost = self
            .cost_basis
            .get(token)
            .and_then(|v| v.as_bigdecimal().to_u128())
            .unwrap_or(0);
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
        token: &TokenAccount,
        sell_amount: u128,
        sell_proceeds_yocto: u128,
    ) -> f64 {
        let total_holding = self
            .holdings
            .get(token)
            .map(|a| to_u128_or_warn(a.smallest_units(), "holdings"))
            .unwrap_or(0)
            + sell_amount;
        let cost_of_sold = self.average_cost_of_sold(token, sell_amount, total_holding);

        // Safety: i128::MAX (~1.7e38 yoctoNEAR = ~1.7e14 NEAR) far exceeds realistic values.
        let pnl = sell_proceeds_yocto as i128 - cost_of_sold as i128;

        // Update cost basis (subtract the sold portion)
        if let Some(basis) = self.cost_basis.get_mut(token) {
            let cost_yocto = YoctoValue::from_yocto(BigDecimal::from(cost_of_sold));
            *basis = basis.saturating_sub(&cost_yocto);
        }

        // Accumulate realized P&L
        self.realized_pnl += pnl;
        *self.realized_pnl_by_token.entry(token.clone()).or_insert(0) += pnl;

        // Clean up cost_basis if position is fully closed
        let remaining = self
            .holdings
            .get(token)
            .map(|a| a.is_zero())
            .unwrap_or(true);
        if remaining {
            self.cost_basis.remove(token);
        }

        // Note: i128 -> f64 loses precision beyond 2^53 yoctoNEAR (~0.009 NEAR); acceptable for metrics.
        pnl as f64 / 1e24
    }

    /// Liquidate all holdings by selling everything back to WNEAR.
    pub async fn liquidate_all(
        &mut self,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> Result<()> {
        let log = DEFAULT.new(o!("function" => "liquidate_all"));

        let tokens: Vec<(TokenAccount, TokenAmount)> = self
            .holdings
            .iter()
            .filter(|&(_, amount)| !amount.is_zero())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        for (token, amount) in tokens {
            let amount_raw = to_u128_or_warn(amount.smallest_units(), "liquidation_amount");
            let sell_yocto = self
                .token_amount_to_yocto(&token, &amount, sim_day, rate_provider)
                .await;

            if sell_yocto == 0 {
                warn!(log, "no rate for liquidation, skipping"; "token" => %token);
                continue;
            }

            self.holdings.remove(&token);
            let sell_yocto_value = YoctoValue::from_yocto(BigDecimal::from(sell_yocto));
            self.cash_balance = self.cash_balance.clone() + sell_yocto_value;

            let pnl_near = self.record_sell_pnl(&token, amount_raw, sell_yocto);

            self.trades.push(TradeRecord {
                timestamp: sim_day,
                action: "liquidation".to_string(),
                token: token.clone(),
                amount,
                price_near: sell_yocto as f64 / 1e24,
                realized_pnl_near: Some(pnl_near),
            });

            trace!(log, "liquidated"; "token" => %token, "amount" => amount_raw, "proceeds" => sell_yocto as f64 / 1e24);
        }

        Ok(())
    }

    /// Convert token amount to yoctoNEAR using rates from provider
    async fn token_amount_to_yocto(
        &self,
        token: &TokenAccount,
        amount: &TokenAmount,
        sim_day: DateTime<Utc>,
        rate_provider: &(impl RateProvider + ?Sized),
    ) -> u128 {
        let token_out = token.to_out();

        match rate_provider.get_rate(&token_out, sim_day).await {
            Some(rate) => {
                if rate.is_zero() {
                    return 0;
                }
                let near_value = amount / &rate;
                // NearValue -> yoctoNEAR
                let yocto = near_value.to_yocto();
                to_u128_or_warn(yocto.as_bigdecimal(), "token_to_yocto")
            }
            None => 0,
        }
    }
}

/// Get the spot rate for a token at (or before) the given date.
///
/// Uses `get_spot_rates_at_time` which finds the latest rate at or before
/// the specified timestamp across the entire DB, so no lookback window is needed.
pub(crate) async fn get_rate_at_date(
    token_out: &TokenOutAccount,
    wnear_in: &common::types::TokenInAccount,
    sim_day: DateTime<Utc>,
) -> Option<ExchangeRate> {
    let tokens = [token_out.clone()];
    let rates = TokenRate::get_spot_rates_at_time(&tokens, wnear_in, sim_day.naive_utc())
        .await
        .ok()?;
    rates.get(token_out).cloned()
}

#[cfg(test)]
mod tests;
