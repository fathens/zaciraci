use super::types::FeeModel;
use bigdecimal::{BigDecimal, FromPrimitive};
use common::stats::ValueAtTime;

/// 取引コストを計算（yoctoNEAR価値ベース - BigDecimal精度保持）
pub fn calculate_trading_cost_by_value_yocto_bd(
    trade_value_yocto: &BigDecimal, // yoctoNEAR建ての取引価値
    fee_model: &FeeModel,
    slippage_rate: &BigDecimal,
    gas_cost_yocto: &BigDecimal, // yoctoNEAR建てのガスコスト
) -> BigDecimal {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => trade_value_yocto * BigDecimal::from_f64(0.003).unwrap_or_default(), // 0.3%
        FeeModel::Zero => BigDecimal::from(0),
        FeeModel::Custom(rate) => {
            trade_value_yocto * BigDecimal::from_f64(*rate).unwrap_or_default()
        }
    };

    let slippage_cost = trade_value_yocto * slippage_rate;

    protocol_fee + slippage_cost + gas_cost_yocto
}

/// 取引コストを計算（yoctoNEAR価値ベース - f64版は後方互換性用）
pub fn calculate_trading_cost_by_value_yocto(
    trade_value_yocto: f64, // yoctoNEAR建ての取引価値
    fee_model: &FeeModel,
    slippage_rate: f64,
    gas_cost_yocto: f64, // yoctoNEAR建てのガスコスト
) -> f64 {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => trade_value_yocto * 0.003, // 0.3%
        FeeModel::Zero => 0.0,
        FeeModel::Custom(rate) => trade_value_yocto * rate,
    };

    let slippage_cost = trade_value_yocto * slippage_rate;

    protocol_fee + slippage_cost + gas_cost_yocto
}

/// 取引コストを計算（価値ベース - 後方互換性用）
pub fn calculate_trading_cost_by_value(
    trade_value: f64, // NEAR建ての取引価値
    fee_model: &FeeModel,
    slippage_rate: f64,
    gas_cost: f64,
) -> f64 {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => trade_value * 0.003, // 0.3%
        FeeModel::Zero => 0.0,
        FeeModel::Custom(rate) => trade_value * rate,
    };

    let slippage_cost = trade_value * slippage_rate;

    protocol_fee + slippage_cost + gas_cost
}

/// 取引コストを計算（後方互換性のため残存）
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

/// Calculate token volatility from price data
pub fn calculate_token_volatility(prices: &[ValueAtTime]) -> f64 {
    common::algorithm::calculate_volatility_from_value_at_time(prices)
}
