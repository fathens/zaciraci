use super::types::*;
use super::utils::calculate_trading_cost;
use anyhow::Result;
use bigdecimal::{BigDecimal, FromPrimitive};
use chrono::{DateTime, Utc};
use common::algorithm::TradingAction;
use std::collections::HashMap;

/// Generate simple mock predictions (placeholder for now)
pub async fn generate_api_predictions(
    _backend_client: &crate::api::backend::BackendClient,
    target_tokens: &[String],
    _quote_token: &str,
    current_time: DateTime<Utc>,
    _historical_days: i64,
    _prediction_horizon: chrono::Duration,
) -> Result<Vec<PredictionData>> {
    let mut predictions = Vec::new();

    // Simple mock predictions for now
    for token in target_tokens {
        predictions.push(PredictionData {
            token: token.clone(),
            current_price: BigDecimal::from_f64(1.0).unwrap_or_default(),
            predicted_price_24h: BigDecimal::from_f64(1.05).unwrap_or_default(), // 5% increase
            timestamp: current_time,
            confidence: Some(0.5),
        });
    }

    Ok(predictions)
}

/// Trading context for managing mutable state during trade execution
pub struct TradeContext<'a> {
    pub current_token: &'a str,
    pub current_amount: f64,
    pub current_price: f64,
    pub all_prices: &'a HashMap<String, f64>,
    pub holdings: &'a mut HashMap<String, f64>,
    pub timestamp: DateTime<Utc>,
    pub config: &'a SimulationConfig,
}

/// Execute a trading action and return the trade execution details
pub fn execute_trading_action(
    action: TradingAction,
    ctx: &mut TradeContext,
) -> Result<Option<TradeExecution>> {
    match action {
        TradingAction::Hold => Ok(None),

        TradingAction::Sell { token: _, target } => {
            let target_price = ctx.all_prices.get(&target).copied().unwrap_or(0.0);
            if target_price <= 0.0 {
                return Ok(None);
            }

            // 取引コストを計算
            let trade_cost = calculate_trading_cost(
                ctx.current_amount,
                &ctx.config.fee_model,
                ctx.config.slippage_rate,
                ctx.config
                    .gas_cost
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.01),
            );

            let net_amount = ctx.current_amount - trade_cost;
            let new_amount = net_amount * ctx.current_price / target_price;

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(target.clone(), new_amount);

            let portfolio_before = ctx.current_amount * ctx.current_price;
            let portfolio_after = new_amount * target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: target,
                amount: ctx.current_amount,
                executed_price: target_price,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from_f64(trade_cost * 0.7).unwrap_or_default(),
                    slippage: BigDecimal::from_f64(trade_cost * 0.2).unwrap_or_default(),
                    gas_fee: ctx.config.gas_cost.clone(),
                    total: BigDecimal::from_f64(trade_cost).unwrap_or_default(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum sell executed".to_string(),
            }))
        }

        TradingAction::Switch { from: _, to } => {
            let target_price = ctx.all_prices.get(&to).copied().unwrap_or(0.0);
            if target_price <= 0.0 {
                return Ok(None);
            }

            // 取引コストを計算
            let trade_cost = calculate_trading_cost(
                ctx.current_amount,
                &ctx.config.fee_model,
                ctx.config.slippage_rate,
                ctx.config
                    .gas_cost
                    .to_string()
                    .parse::<f64>()
                    .unwrap_or(0.01),
            );

            let net_amount = ctx.current_amount - trade_cost;
            let new_amount = net_amount * ctx.current_price / target_price;

            // ポートフォリオ更新
            ctx.holdings.remove(ctx.current_token);
            ctx.holdings.insert(to.clone(), new_amount);

            let portfolio_before = ctx.current_amount * ctx.current_price;
            let portfolio_after = new_amount * target_price;

            Ok(Some(TradeExecution {
                timestamp: ctx.timestamp,
                from_token: ctx.current_token.to_string(),
                to_token: to,
                amount: ctx.current_amount,
                executed_price: target_price,
                cost: TradingCost {
                    protocol_fee: BigDecimal::from_f64(trade_cost * 0.7).unwrap_or_default(),
                    slippage: BigDecimal::from_f64(trade_cost * 0.2).unwrap_or_default(),
                    gas_fee: ctx.config.gas_cost.clone(),
                    total: BigDecimal::from_f64(trade_cost).unwrap_or_default(),
                },
                portfolio_value_before: portfolio_before,
                portfolio_value_after: portfolio_after,
                success: true,
                reason: "Momentum switch executed".to_string(),
            }))
        }

        // 新しいアクションタイプの処理（今回はプレースホルダーとして）
        TradingAction::Rebalance { .. } => {
            // ポートフォリオリバランスの処理（将来実装）
            Ok(None)
        }
        TradingAction::AddPosition { .. } => {
            // ポジション追加の処理（将来実装）
            Ok(None)
        }
        TradingAction::ReducePosition { .. } => {
            // ポジション削減の処理（将来実装）
            Ok(None)
        }
    }
}

/// Immutable portfolio operations for functional trading
impl ImmutablePortfolio {
    /// Create a new portfolio with initial capital in a specific token
    pub fn new(initial_capital: f64, initial_token: &str) -> Self {
        let mut holdings = HashMap::new();
        holdings.insert(initial_token.to_string(), initial_capital);

        Self {
            holdings,
            cash_balance: 0.0,
            timestamp: Utc::now(),
        }
    }

    /// Calculate total portfolio value using market prices
    pub fn total_value(&self, market: &MarketSnapshot) -> f64 {
        let mut total = self.cash_balance;

        for (token, amount) in &self.holdings {
            if let Some(&price) = market.prices.get(token) {
                total += amount * price;
            }
        }

        total
    }

    /// Apply a trading decision and return the portfolio transition
    pub fn apply_trade(
        &self,
        decision: &TradingDecision,
        market: &MarketSnapshot,
        _config: &TradingConfig,
    ) -> Result<PortfolioTransition> {
        let mut new_holdings = self.holdings.clone();
        let mut cost = 0.0;

        let new_portfolio = match decision {
            TradingDecision::Hold => ImmutablePortfolio {
                holdings: new_holdings,
                cash_balance: self.cash_balance,
                timestamp: market.timestamp,
            },
            TradingDecision::Sell { target_token } => {
                // Sell current holding to target token
                if let Some((current_token, current_amount)) = new_holdings.iter().next() {
                    let current_token = current_token.clone();
                    let current_amount = *current_amount;

                    new_holdings.remove(&current_token);

                    if let Some(&target_price) = market.prices.get(target_token) {
                        let target_amount = current_amount / target_price;
                        cost = current_amount * 0.006; // Simple fee calculation
                        let net_amount = target_amount - (cost / target_price);

                        new_holdings.insert(target_token.clone(), net_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
            TradingDecision::Switch { from, to } => {
                if let Some(&from_amount) = new_holdings.get(from) {
                    new_holdings.remove(from);

                    if let (Some(&from_price), Some(&to_price)) =
                        (market.prices.get(from), market.prices.get(to))
                    {
                        let from_value = from_amount * from_price;
                        cost = from_value * 0.006; // Simple fee calculation
                        let net_value = from_value - cost;
                        let to_amount = net_value / to_price;

                        new_holdings.insert(to.clone(), to_amount);
                    }
                }

                ImmutablePortfolio {
                    holdings: new_holdings,
                    cash_balance: self.cash_balance,
                    timestamp: market.timestamp,
                }
            }
        };

        Ok(PortfolioTransition {
            from: self.clone(),
            to: new_portfolio,
            action: decision.clone(),
            cost,
            reason: format!("Trade executed: {:?}", decision),
        })
    }

    /// Get the dominant token in the portfolio (token with highest value)
    pub fn get_dominant_token(&self, market: &MarketSnapshot) -> Option<String> {
        let mut max_value = 0.0;
        let mut dominant_token = None;

        for (token, amount) in &self.holdings {
            if let Some(&price) = market.prices.get(token) {
                let value = amount * price;
                if value > max_value {
                    max_value = value;
                    dominant_token = Some(token.clone());
                }
            }
        }

        dominant_token
    }

    /// Check if portfolio has exposure to a specific token
    pub fn has_token(&self, token: &str) -> bool {
        self.holdings.contains_key(token) && self.holdings[token] > 0.0
    }

    /// Get allocation percentage for each token
    pub fn get_allocations(&self, market: &MarketSnapshot) -> HashMap<String, f64> {
        let total_value = self.total_value(market);
        let mut allocations = HashMap::new();

        if total_value > 0.0 {
            for (token, amount) in &self.holdings {
                if let Some(&price) = market.prices.get(token) {
                    let token_value = amount * price;
                    let allocation = (token_value / total_value) * 100.0;
                    allocations.insert(token.clone(), allocation);
                }
            }
        }

        // Add cash allocation
        if self.cash_balance > 0.0 {
            let cash_allocation = (self.cash_balance / total_value) * 100.0;
            allocations.insert("cash".to_string(), cash_allocation);
        }

        allocations
    }

    /// Calculate portfolio risk metrics
    pub fn calculate_portfolio_risk(&self, market: &MarketSnapshot) -> PortfolioRisk {
        let allocations = self.get_allocations(market);
        let num_positions = self.holdings.len();

        // Calculate concentration risk (Herfindahl index)
        let concentration_index: f64 = allocations
            .values()
            .map(|&allocation| (allocation / 100.0).powi(2))
            .sum();

        // Calculate diversification score (1 - concentration index)
        let diversification_score = 1.0 - concentration_index;

        // Risk level based on concentration
        let risk_level = if concentration_index > 0.8 {
            RiskLevel::High
        } else if concentration_index > 0.5 {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        };

        PortfolioRisk {
            concentration_index,
            diversification_score,
            num_positions,
            risk_level,
            largest_position_pct: allocations.values().fold(0.0f64, |a, &b| a.max(b)),
        }
    }
}

/// Portfolio risk assessment
#[derive(Debug, Clone)]
pub struct PortfolioRisk {
    pub concentration_index: f64,
    pub diversification_score: f64,
    pub num_positions: usize,
    pub risk_level: RiskLevel,
    pub largest_position_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Trading strategy implementations
impl TradingStrategy for MomentumStrategy {
    fn name(&self) -> &'static str {
        "Momentum"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        // Filter opportunities by minimum confidence
        let high_confidence_opportunities: Vec<&TokenOpportunity> = opportunities
            .iter()
            .filter(|opp| opp.confidence.unwrap_or(0.0) >= self.min_confidence)
            .collect();

        if high_confidence_opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        // Sort by confidence-adjusted expected return
        let mut sorted_opportunities = high_confidence_opportunities;
        sorted_opportunities.sort_by(|a, b| {
            let score_a = a.expected_return * a.confidence.unwrap_or(0.5);
            let score_b = b.expected_return * b.confidence.unwrap_or(0.5);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let best_opportunity = sorted_opportunities.first().unwrap();

        // Check if we should switch to the best opportunity
        if let Some(current_token) = portfolio.get_dominant_token(market) {
            if current_token != best_opportunity.token {
                let current_opportunity =
                    opportunities.iter().find(|opp| opp.token == current_token);

                if let Some(current_opp) = current_opportunity {
                    let current_score =
                        current_opp.expected_return * current_opp.confidence.unwrap_or(0.5);
                    let best_score = best_opportunity.expected_return
                        * best_opportunity.confidence.unwrap_or(0.5);

                    if best_score > current_score * config.switch_multiplier {
                        return Ok(TradingDecision::Switch {
                            from: current_token,
                            to: best_opportunity.token.clone(),
                        });
                    }
                }
            }
        } else if best_opportunity.expected_return > config.min_profit_threshold {
            // No current position, enter best opportunity if profitable enough
            return Ok(TradingDecision::Sell {
                target_token: best_opportunity.token.clone(),
            });
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        // Momentum strategy rebalances based on lookback periods
        true // Simplified implementation
    }
}

impl TradingStrategy for PortfolioStrategy {
    fn name(&self) -> &'static str {
        "Portfolio"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        _config: &TradingConfig,
    ) -> Result<TradingDecision> {
        let current_positions = portfolio.holdings.len();

        // If we have fewer positions than max, consider adding
        if current_positions < self.max_positions {
            // Find the best opportunity not currently held
            let best_new_opportunity = opportunities
                .iter()
                .filter(|opp| !portfolio.has_token(&opp.token))
                .max_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let Some(opp) = best_new_opportunity {
                if opp.expected_return > 0.05 {
                    // 5% minimum expected return
                    return Ok(TradingDecision::Sell {
                        target_token: opp.token.clone(),
                    });
                }
            }
        }

        // Check if we should rebalance existing positions
        if self.should_rebalance(portfolio, market) {
            // Find the worst performing position to potentially replace
            let worst_position = opportunities
                .iter()
                .filter(|opp| portfolio.has_token(&opp.token))
                .min_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            let best_opportunity = opportunities
                .iter()
                .filter(|opp| !portfolio.has_token(&opp.token))
                .max_by(|a, b| {
                    a.expected_return
                        .partial_cmp(&b.expected_return)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

            if let (Some(worst), Some(best)) = (worst_position, best_opportunity) {
                if best.expected_return > worst.expected_return * 1.2 {
                    // 20% improvement threshold
                    return Ok(TradingDecision::Switch {
                        from: worst.token.clone(),
                        to: best.token.clone(),
                    });
                }
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, portfolio: &ImmutablePortfolio, market: &MarketSnapshot) -> bool {
        let allocations = portfolio.get_allocations(market);
        let target_allocation = 100.0 / self.max_positions as f64;

        // Check if any allocation deviates significantly from target
        allocations
            .values()
            .any(|&allocation| (allocation - target_allocation).abs() > self.rebalance_threshold)
    }
}

impl TradingStrategy for TrendFollowingStrategy {
    fn name(&self) -> &'static str {
        "TrendFollowing"
    }

    fn make_decision(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        // Filter opportunities with strong trends and low volatility
        let trend_opportunities: Vec<&TokenOpportunity> = opportunities
            .iter()
            .filter(|opp| {
                opp.expected_return > 0.02 && // Minimum 2% expected return for trend
                opp.confidence.unwrap_or(0.0) > 0.6 // High confidence in trend
            })
            .collect();

        if trend_opportunities.is_empty() {
            return Ok(TradingDecision::Hold);
        }

        // Sort by trend strength (expected return weighted by confidence)
        let mut sorted_trends = trend_opportunities;
        sorted_trends.sort_by(|a, b| {
            let strength_a = a.expected_return * a.confidence.unwrap_or(0.5);
            let strength_b = b.expected_return * b.confidence.unwrap_or(0.5);
            strength_b
                .partial_cmp(&strength_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let strongest_trend = sorted_trends.first().unwrap();

        // Trend following typically uses concentrated positions
        if let Some(current_token) = portfolio.get_dominant_token(market) {
            if current_token != strongest_trend.token {
                let trend_strength =
                    strongest_trend.expected_return * strongest_trend.confidence.unwrap_or(0.5);

                if trend_strength > config.min_profit_threshold * 2.0 {
                    return Ok(TradingDecision::Switch {
                        from: current_token,
                        to: strongest_trend.token.clone(),
                    });
                }
            }
        } else {
            let trend_strength =
                strongest_trend.expected_return * strongest_trend.confidence.unwrap_or(0.5);

            if trend_strength > config.min_profit_threshold {
                return Ok(TradingDecision::Sell {
                    target_token: strongest_trend.token.clone(),
                });
            }
        }

        Ok(TradingDecision::Hold)
    }

    fn should_rebalance(&self, _portfolio: &ImmutablePortfolio, _market: &MarketSnapshot) -> bool {
        // Trend following typically holds positions longer
        false // Simplified implementation - only rebalance on signal changes
    }
}

/// Strategy context for managing different trading strategies
impl StrategyContext {
    /// Create a new strategy context with the specified strategy
    pub fn new(strategy: Box<dyn TradingStrategy>) -> Self {
        Self { strategy }
    }

    /// Execute the strategy's decision-making process
    pub fn execute_strategy(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
        opportunities: &[TokenOpportunity],
        config: &TradingConfig,
    ) -> Result<TradingDecision> {
        self.strategy
            .make_decision(portfolio, market, opportunities, config)
    }

    /// Get the name of the current strategy
    pub fn strategy_name(&self) -> &'static str {
        self.strategy.name()
    }

    /// Check if the strategy recommends rebalancing
    pub fn should_rebalance(
        &self,
        portfolio: &ImmutablePortfolio,
        market: &MarketSnapshot,
    ) -> bool {
        self.strategy.should_rebalance(portfolio, market)
    }
}

/// Prediction data structure for API predictions
#[derive(Debug, Clone)]
pub struct PredictionData {
    pub token: String,
    pub current_price: BigDecimal,
    pub predicted_price_24h: BigDecimal,
    pub timestamp: DateTime<Utc>,
    pub confidence: Option<f64>,
}

/// Convert PredictionData to TokenOpportunity for strategy use
impl From<&PredictionData> for TokenOpportunity {
    fn from(prediction: &PredictionData) -> Self {
        let current_price = prediction
            .current_price
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);
        let predicted_price = prediction
            .predicted_price_24h
            .to_string()
            .parse::<f64>()
            .unwrap_or(0.0);

        let expected_return = if current_price > 0.0 {
            (predicted_price - current_price) / current_price
        } else {
            0.0
        };

        TokenOpportunity {
            token: prediction.token.clone(),
            expected_return,
            confidence: prediction.confidence,
        }
    }
}
