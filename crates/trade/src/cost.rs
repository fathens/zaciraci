//! 取引コストの事前見積もり
//!
//! Markowitz 最適化に渡す前に expected_return から AMM 手数料・price impact・
//! ガス・storage・スリッページマージンを差し引くための純関数群。

use crate::Result;
use bigdecimal::{BigDecimal, ToPrimitive, Zero};
use blockchain::ref_finance::path::preview::estimate_swap_gas_cost_yocto;
use blockchain::types::gas_price::GasPrice;
use common::types::{ExchangeRate, NearValue, TokenAmount, YoctoValue};
use dex::TokenPath;

/// 期待リターンから事前控除するスリッページマージン
///
/// `slippage::MIN_SLIPPAGE_BUDGET` (実行時 min_out 用) と意味論的に独立。
/// こちらは「事前 cost 推定」目的で、DB データ鮮度・他トレーダー・ブロック間
/// 価格変動による期待外れを保守的に吸収する。
pub const EXPECTED_SLIPPAGE_DEDUCTION: f64 = 0.005;

/// 取引コストの内訳
///
/// - `variable_ratio`: AMM fee + price impact + slippage（取引額に比例しない比率部分）
/// - `fixed_cost`: gas + storage（取引したら掛かる固定費、yoctoNEAR 単位）
#[derive(Debug, Clone)]
pub struct TradeCostBreakdown {
    variable_ratio: f64,
    fixed_cost: YoctoValue,
}

impl TradeCostBreakdown {
    /// 比率部分（AMM fee + price impact + slippage margin）
    pub fn variable_ratio(&self) -> f64 {
        self.variable_ratio
    }

    /// 固定費（gas + storage）
    pub fn fixed_cost(&self) -> &YoctoValue {
        &self.fixed_cost
    }

    /// 期待リターン ratio から差し引く net deduction を計算する。
    ///
    /// `assumed_position`: スワップする入力金額の見積もり（yoctoNEAR）
    ///
    /// `assumed_position` が 0 の場合は `f64::INFINITY` を返す。この値は
    /// 呼び出し元で `is_finite()` フィルタや
    /// `PortfolioData::retain_tokens` 経由で除外されてから Markowitz の
    /// `expected_return` から減算される前提。
    ///
    /// 共分散ソルバ (`common::algorithm::portfolio::box_maximize_sharpe`) に
    /// 直接渡すと、Cholesky 後段で `0 × INFINITY = NaN` 連鎖が生じる。NaN は
    /// `<` `>` のすべての比較で false になるため、`sum_p.abs() < 1e-15` 等の
    /// ガード（`portfolio.rs:739` 等）はすべて防御失効し、サイレント Hold
    /// （rebalance_needed=false）が再生される。
    pub fn to_return_deduction(&self, assumed_position: &YoctoValue) -> f64 {
        if assumed_position.as_bigdecimal().is_zero() {
            return f64::INFINITY;
        }
        // NEAR スケールで f64 変換 (~10⁻³ オーダー → f64 仮数部範囲内)
        let fixed_near = self.fixed_cost.to_near();
        let position_near = assumed_position.to_near();
        let ratio = (fixed_near.as_bigdecimal() / position_near.as_bigdecimal())
            .to_f64()
            .unwrap_or(0.0);
        self.variable_ratio + ratio
    }
}

/// 与えられたパスでの取引コスト見積もり
///
/// - **variable_ratio**: `assumed_in × spot_rate - path.calc_value(assumed_in)`
///   から AMM fee + price impact 一括計算 + `EXPECTED_SLIPPAGE_DEDUCTION` 加算
/// - **fixed_cost**: `estimate_swap_gas_cost_yocto(gas_price, depth)` + `storage_min × new_token_count`
///
/// `assumed_in` が 0 の場合は price impact が計測できないため、variable_ratio は
/// `EXPECTED_SLIPPAGE_DEDUCTION` のみ。
pub fn estimate_trade_cost(
    path: &TokenPath,
    assumed_in: &YoctoValue,
    spot_rate: &ExchangeRate,
    gas_price: GasPrice,
    storage_min_per_token: &YoctoValue,
    new_token_count: usize,
) -> Result<TradeCostBreakdown> {
    let depth = path.len();

    let variable_ratio = compute_variable_ratio(path, assumed_in, spot_rate)?;

    let gas_yocto = estimate_swap_gas_cost_yocto(gas_price, depth);
    let storage_count = u128::try_from(new_token_count).unwrap_or(u128::MAX);
    let storage_per_token = storage_min_per_token
        .as_bigdecimal()
        .to_u128()
        .unwrap_or(u128::MAX);
    let storage_yocto = storage_per_token.saturating_mul(storage_count);
    let gas_u128 = gas_yocto.as_bigdecimal().to_u128().unwrap_or(0);
    let fixed_yocto = gas_u128.saturating_add(storage_yocto);

    Ok(TradeCostBreakdown {
        variable_ratio,
        fixed_cost: YoctoValue::from_yocto_u128(fixed_yocto),
    })
}

/// `assumed_in` を path に通したときの実効的な loss ratio
///
/// `(input_NEAR - output_NEAR_via_spot_rate) / input_NEAR` で AMM fee と price
/// impact を一括計算し、`EXPECTED_SLIPPAGE_DEDUCTION` を加算して返す。
fn compute_variable_ratio(
    path: &TokenPath,
    assumed_in: &YoctoValue,
    spot_rate: &ExchangeRate,
) -> Result<f64> {
    let assumed_in_yocto = assumed_in
        .as_bigdecimal()
        .to_u128()
        .ok_or_else(|| anyhow::anyhow!("assumed_in too large to convert to u128"))?;
    if assumed_in_yocto == 0 {
        return Ok(EXPECTED_SLIPPAGE_DEDUCTION);
    }

    let output_smallest = path.calc_value(assumed_in_yocto)?;
    let output_amount =
        TokenAmount::from_smallest_units(BigDecimal::from(output_smallest), spot_rate.decimals());
    let output_near = (&output_amount) / spot_rate;
    let input_near = assumed_in.to_near();

    let amm_loss = compute_loss_ratio(&input_near, &output_near);
    Ok(amm_loss + EXPECTED_SLIPPAGE_DEDUCTION)
}

/// `(input - output) / input` を非負クランプして f64 で返す
///
/// 数値誤差で出力が入力をわずかに上回る（負の loss）ケースは 0.0 にクランプ。
fn compute_loss_ratio(input_near: &NearValue, output_near: &NearValue) -> f64 {
    let input_bd = input_near.as_bigdecimal();
    if input_bd <= &BigDecimal::zero() {
        return 0.0;
    }
    let output_bd = output_near.as_bigdecimal();
    ((input_bd - output_bd) / input_bd)
        .to_f64()
        .unwrap_or(0.0)
        .max(0.0)
}

#[cfg(test)]
mod tests;
