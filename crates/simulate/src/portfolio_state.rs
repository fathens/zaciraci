use anyhow::Result;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use chrono::{DateTime, Utc};
use common::types::{ExchangeRate, TokenAccount, TokenAmount, TokenOutAccount, YoctoValue};
use logging::*;
use persistence::token_rate::TokenRate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::mem;

/// 非負の BigDecimal を u128 に変換する。
///
/// 変換できない場合（負値、小数部のみ、u128 範囲超過）は warn ログを出力し 0 を返す。
/// 呼び出し元は 0 が返った場合に処理をスキップするガードを持つ前提で設計されている。
pub(crate) fn to_u128_or_warn(value: &BigDecimal, context: &str) -> u128 {
    value.to_u128().unwrap_or_else(|| {
        let log = DEFAULT.new(o!("function" => "to_u128_or_warn"));
        warn!(log, "BigDecimal value exceeds u128 range, defaulting to 0";
            "context" => context, "value" => %value);
        0
    })
}

/// BigDecimal を i128 に変換する。
///
/// 変換できない場合（小数部が残る、i128 範囲超過）は warn ログを出力し 0 を返す。
pub(crate) fn to_i128_or_warn(value: &BigDecimal, context: &str) -> i128 {
    value.to_i128().unwrap_or_else(|| {
        let log = DEFAULT.new(o!("function" => "to_i128_or_warn"));
        warn!(log, "BigDecimal value cannot be converted to i128, defaulting to 0";
            "context" => context, "value" => %value);
        0
    })
}

/// BigDecimal を f64 に変換する。
///
/// 変換できない場合（Infinity、NaN 等）は warn ログを出力し 0.0 を返す。
pub(crate) fn to_f64_or_warn(value: &BigDecimal, context: &str) -> f64 {
    value.to_f64().unwrap_or_else(|| {
        let log = DEFAULT.new(o!("function" => "to_f64_or_warn"));
        warn!(log, "BigDecimal value cannot be converted to f64, defaulting to 0.0";
            "context" => context, "value" => %value);
        0.0
    })
}

/// Convert yoctoNEAR (i128) to NEAR (f64) for display metrics.
///
/// i128 → f64 loses precision beyond 2^53 yoctoNEAR (~9 nanoNEAR);
/// acceptable for display metrics where sub-nanoNEAR accuracy is irrelevant.
pub(crate) fn pnl_to_near(pnl_yocto: i128) -> f64 {
    pnl_yocto as f64 / 1e24
}

/// Convert yoctoNEAR (u128) to NEAR (f64) for display metrics.
///
/// u128 → f64 loses precision beyond 2^53 yoctoNEAR (~9 nanoNEAR);
/// acceptable for display metrics where sub-nanoNEAR accuracy is irrelevant.
pub(crate) fn yocto_to_near(yocto: u128) -> f64 {
    yocto as f64 / 1e24
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

/// Actual amounts of a successful simulated swap after balance clamping.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SwapResult {
    pub(crate) actual_in: u128,
    pub(crate) actual_out: u128,
}

/// Method used for swap output calculation during simulation.
///
/// `pub` because it is used in public output types (`SwapEventEntry`, `SwapStats`)
/// defined in the `output` module.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SwapMethod {
    /// Pool-based estimate_return (fee + slippage aware)
    PoolBased,
    /// Fallback to DB rate conversion (no fee/slippage)
    DbRate,
}

/// Record of a single swap operation during simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SwapEvent {
    pub(crate) timestamp: DateTime<Utc>,
    pub(crate) token_in: TokenAccount,
    pub(crate) amount_in: TokenAmount,
    pub(crate) token_out: TokenAccount,
    pub(crate) amount_out: TokenAmount,
    pub(crate) swap_method: SwapMethod,
    pub(crate) pool_ids: Vec<u32>,
}

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
    /// swap event history
    pub(crate) swap_events: Vec<SwapEvent>,
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
            swap_events: Vec::new(),
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
    ) -> Option<SwapResult> {
        let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;

        // Get the available balance for the source token (computed once, reused
        // for both clamping and P&L cost-basis lookup).
        let from_balance = if from_token == wnear {
            to_u128_or_warn(self.cash_balance.as_bigdecimal(), "cash_balance")
        } else {
            self.holdings
                .get(from_token)
                .map(|a| to_u128_or_warn(a.smallest_units(), "holdings"))
                .unwrap_or(0)
        };

        // Clamp from_amount to available balance and proportionally scale to_amount
        // to maintain consistent input/output ratio.
        let actual_from = from_amount.min(from_balance);
        if actual_from == 0 {
            return None;
        }
        let actual_to = Self::scale_output(to_amount, actual_from, from_amount);
        if actual_to == 0 {
            return None;
        }

        let from_yocto = YoctoValue::from_yocto(BigDecimal::from(actual_from));
        let to_yocto = YoctoValue::from_yocto(BigDecimal::from(actual_to));

        // Pre-compute cost basis transfer for direct token-to-token swaps.
        // IMPORTANT: Must be done before record_sell_pnl modifies cost_basis.
        // This value is also reused as sell_proceeds_yocto for non-WNEAR destinations,
        // avoiding a duplicate call to average_cost_of_sold.
        let transferred_cost = if from_token != wnear && to_token != wnear {
            self.average_cost_of_sold(from_token, actual_from, from_balance)
        } else {
            YoctoValue::zero()
        };

        // Deduct from source
        if from_token == wnear {
            self.cash_balance = self.cash_balance.saturating_sub(&from_yocto);
        } else {
            // Subtract from holdings
            if let Some(holding) = self.holdings.get_mut(from_token) {
                let new_units = (holding.smallest_units() - BigDecimal::from(actual_from))
                    .max(BigDecimal::zero());
                *holding = TokenAmount::from_smallest_units(new_units, holding.decimals());
            }

            // Determine sell proceeds for P&L calculation.
            let sell_proceeds_yocto = if to_token == wnear {
                YoctoValue::from_yocto(BigDecimal::from(actual_to))
            } else {
                transferred_cost.clone()
            };

            self.record_sell_pnl(from_token, actual_from, &sell_proceeds_yocto);

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
            let balance = mem::replace(&mut self.cash_balance, YoctoValue::zero());
            self.cash_balance = balance + to_yocto;
        } else {
            let entry = self.holdings.entry(to_token.clone()).or_insert_with(|| {
                // Use decimals from existing holdings or default to 24
                TokenAmount::zero(DEFAULT_DECIMALS)
            });
            let new_units = entry.smallest_units() + BigDecimal::from(actual_to);
            *entry = TokenAmount::from_smallest_units(new_units, entry.decimals());

            // Track cost basis: the NEAR value of what we spent
            if from_token == wnear {
                self.add_to_cost_basis(to_token, from_yocto);
            } else if !transferred_cost.is_zero() {
                // Direct token-to-token swap: transfer proportional cost basis
                // from the sold token to the acquired token.
                self.add_to_cost_basis(to_token, transferred_cost);
            }
        }

        Some(SwapResult {
            actual_in: actual_from,
            actual_out: actual_to,
        })
    }

    /// Add `amount` to the cost basis of `token`.
    fn add_to_cost_basis(&mut self, token: &TokenAccount, amount: YoctoValue) {
        let basis = self
            .cost_basis
            .entry(token.clone())
            .or_insert_with(YoctoValue::zero);
        let current = mem::replace(basis, YoctoValue::zero());
        *basis = current + amount;
    }

    /// Scale output amount proportionally when actual input is less than requested.
    ///
    /// Computes `to_amount * actual / requested` using BigDecimal for precision.
    /// The result is truncated (floor) to the nearest integer, which means the
    /// output is always rounded in the conservative direction (less output).
    ///
    /// # Preconditions
    /// - `requested > 0` (guaranteed by callers which return early when `actual == 0`,
    ///   and `actual <= requested` by construction via `min()`).
    fn scale_output(to_amount: u128, actual: u128, requested: u128) -> u128 {
        debug_assert!(requested > 0, "scale_output called with requested == 0");
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
            realized_pnl_near: pnl_to_near(self.realized_pnl),
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
        let mut total =
            to_f64_or_warn(self.cash_balance.as_bigdecimal(), "cash_balance") / yocto_per_near;

        // Holdings
        for (token_account, token_amount) in &self.holdings {
            if token_amount.is_zero() {
                continue;
            }

            let token_out: TokenOutAccount = token_account.to_out();

            match rate_provider.get_rate(&token_out, sim_day).await {
                Some(rate) => {
                    let near_value = token_amount / &rate;
                    let near_f64: f64 =
                        to_f64_or_warn(near_value.as_bigdecimal(), "token_near_value");
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
    /// Uses `BigDecimal` internally to avoid overflow and precision loss from integer
    /// division that the previous `u128`-based implementation suffered from.
    fn average_cost_of_sold(
        &self,
        token: &TokenAccount,
        sell_amount: u128,
        total_holding: u128,
    ) -> YoctoValue {
        let total_cost = self
            .cost_basis
            .get(token)
            .cloned()
            .unwrap_or_else(YoctoValue::zero);
        if total_cost.is_zero() || sell_amount == 0 {
            YoctoValue::zero()
        } else if sell_amount == total_holding {
            total_cost
        } else if total_holding > 0 {
            // BigDecimal multiplication/division: no overflow, full precision.
            // Down = truncate toward zero. For cost basis (always non-negative) this
            // is equivalent to Floor. The result is conservative: slightly underestimates
            // the cost of the sold portion, which slightly overestimates realized P&L.
            // The error is sub-yoctoNEAR and self-corrects on final sell (early return above).
            let result = (total_cost.as_bigdecimal() * BigDecimal::from(sell_amount))
                / BigDecimal::from(total_holding);
            YoctoValue::from_yocto(result.with_scale_round(0, bigdecimal::RoundingMode::Down))
        } else {
            YoctoValue::zero()
        }
    }

    /// Record realized P&L for a sell operation using average cost basis method.
    /// Returns the realized P&L in NEAR (f64).
    fn record_sell_pnl(
        &mut self,
        token: &TokenAccount,
        sell_amount: u128,
        sell_proceeds_yocto: &YoctoValue,
    ) -> f64 {
        let total_holding = self
            .holdings
            .get(token)
            .map(|a| to_u128_or_warn(a.smallest_units(), "holdings"))
            .unwrap_or(0)
            + sell_amount;
        let cost_of_sold = self.average_cost_of_sold(token, sell_amount, total_holding);

        // P&L = proceeds - cost (using BigDecimal for precision).
        // Down = truncate toward zero: positive P&L is slightly underestimated,
        // negative P&L is slightly underestimated in magnitude (loss looks smaller).
        // Both effects are sub-yoctoNEAR and negligible.
        let pnl_bd = (sell_proceeds_yocto.as_bigdecimal() - cost_of_sold.as_bigdecimal())
            .with_scale_round(0, bigdecimal::RoundingMode::Down);
        let pnl = to_i128_or_warn(&pnl_bd, "realized_pnl");

        // Update cost basis (subtract the sold portion)
        if let Some(basis) = self.cost_basis.get_mut(token) {
            *basis = basis.saturating_sub(&cost_of_sold);
        }

        // Accumulate realized P&L (saturating to prevent silent wraparound in release builds)
        self.realized_pnl = self.realized_pnl.saturating_add(pnl);
        let token_pnl = self.realized_pnl_by_token.entry(token.clone()).or_insert(0);
        *token_pnl = token_pnl.saturating_add(pnl);

        // Clean up cost_basis if position is fully closed
        let remaining = self
            .holdings
            .get(token)
            .map(|a| a.is_zero())
            .unwrap_or(true);
        if remaining {
            self.cost_basis.remove(token);
        }

        pnl_to_near(pnl)
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

            let pnl_near = self.record_sell_pnl(&token, amount_raw, &sell_yocto_value);

            let balance = mem::replace(&mut self.cash_balance, YoctoValue::zero());
            self.cash_balance = balance + sell_yocto_value;

            self.trades.push(TradeRecord {
                timestamp: sim_day,
                action: "liquidation".to_string(),
                token: token.clone(),
                amount,
                price_near: yocto_to_near(sell_yocto),
                realized_pnl_near: Some(pnl_near),
            });

            trace!(log, "liquidated"; "token" => %token, "amount" => amount_raw, "proceeds" => yocto_to_near(sell_yocto));
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
