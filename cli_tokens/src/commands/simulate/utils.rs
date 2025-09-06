use super::types::{FeeModel, TokenOpportunity, TradingConfig, TradingDecision};
use common::algorithm::momentum::TradingAction;
use common::stats::ValueAtTime;

/// 取引コストを計算
pub fn calculate_trading_cost(
    amount: f64,
    fee_model: &FeeModel,
    slippage_rate: f64,
    gas_cost: f64,
) -> f64 {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => amount * 0.003, // 0.3%
        FeeModel::Zero => 0.0,
        FeeModel::Custom(rate) => amount * rate,
    };

    let slippage_cost = amount * slippage_rate;

    protocol_fee + slippage_cost + gas_cost
}

/// Pure function for trading decisions with better testability
pub fn make_trading_decision(
    current_token: &str,
    current_return: f64,
    ranked_opportunities: &[TokenOpportunity],
    holding_amount: f64,
    config: &TradingConfig,
) -> TradingDecision {
    if ranked_opportunities.is_empty() {
        return TradingDecision::Hold;
    }

    let best_opportunity = &ranked_opportunities[0];

    if best_opportunity.token == current_token {
        return TradingDecision::Hold;
    }

    if holding_amount < config.min_trade_amount {
        return TradingDecision::Hold;
    }

    if current_return < config.min_profit_threshold {
        return TradingDecision::Sell {
            target_token: best_opportunity.token.clone(),
        };
    }

    let confidence_adjusted_return =
        best_opportunity.expected_return * best_opportunity.confidence.unwrap_or(0.5);

    if confidence_adjusted_return > current_return * config.switch_multiplier {
        return TradingDecision::Switch {
            from: current_token.to_string(),
            to: best_opportunity.token.clone(),
        };
    }

    TradingDecision::Hold
}

/// Helper function to convert old format to new format for gradual migration
pub fn convert_ranked_tokens_to_opportunities(
    ranked_tokens: &[(String, f64, Option<f64>)],
) -> Vec<TokenOpportunity> {
    ranked_tokens
        .iter()
        .map(|(token, expected_return, confidence)| TokenOpportunity {
            token: token.clone(),
            expected_return: *expected_return,
            confidence: *confidence,
        })
        .collect()
}

/// Helper function to convert TradingDecision back to TradingAction for backward compatibility
pub fn convert_decision_to_action(decision: TradingDecision, current_token: &str) -> TradingAction {
    match decision {
        TradingDecision::Hold => TradingAction::Hold,
        TradingDecision::Sell { target_token } => TradingAction::Sell {
            token: current_token.to_string(),
            target: target_token,
        },
        TradingDecision::Switch { from, to } => TradingAction::Switch { from, to },
    }
}

/// Calculate token volatility from price data
pub fn calculate_token_volatility(prices: &[ValueAtTime]) -> f64 {
    common::algorithm::calculate_volatility_from_value_at_time(prices)
}
