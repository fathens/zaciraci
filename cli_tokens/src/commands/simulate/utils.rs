use super::types::{FeeModel, TradingCost};
use common::stats::ValueAtTime;
use common::types::YoctoValueF64;

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

/// 取引コストを計算し TradingCost 構造体を返す（型安全版）
///
/// 型安全な YoctoValueF64 を使用してコストを計算する。
/// すべての値は yoctoNEAR 単位で表現される。
///
/// # 引数
/// - `trade_value`: 取引価値（yoctoNEAR）
/// - `fee_model`: 手数料モデル
/// - `slippage_rate`: スリッページ率（0.0-1.0）
/// - `gas_cost`: ガスコスト（yoctoNEAR）
pub fn calculate_trading_cost_yocto(
    trade_value: YoctoValueF64,
    fee_model: &FeeModel,
    slippage_rate: f64,
    gas_cost: YoctoValueF64,
) -> TradingCost {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => trade_value * 0.003, // 0.3%
        FeeModel::Zero => YoctoValueF64::zero(),
        FeeModel::Custom(rate) => trade_value * *rate,
    };

    let slippage = trade_value * slippage_rate;
    let total = protocol_fee + slippage + gas_cost;

    TradingCost {
        protocol_fee,
        slippage,
        gas_fee: gas_cost,
        total,
    }
}

/// Calculate token volatility from price data
pub fn calculate_token_volatility(prices: &[ValueAtTime]) -> f64 {
    common::algorithm::calculate_volatility_from_value_at_time(prices)
}
