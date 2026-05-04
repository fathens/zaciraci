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
use std::fmt;

/// 期待リターンから事前控除するスリッページマージン
///
/// `slippage::MIN_SLIPPAGE_BUDGET` (実行時 min_out 用) と意味論的に独立。
/// こちらは「事前 cost 推定」目的で、DB データ鮮度・他トレーダー・ブロック間
/// 価格変動による期待外れを保守的に吸収する。
///
/// # 警告: `f64::INFINITY` を直接 Markowitz に渡してはならない
///
/// `box_maximize_sharpe` の Cholesky 後段で `0 × INFINITY = NaN` 連鎖が生じ、
/// `<` `>` のすべての比較で false になる NaN によりガード（`portfolio.rs:739`
/// 等の `sum_p.abs() < 1e-15`）はすべて防御失効する。`CostDeduction::new` で
/// 不変条件 `is_finite() && >= 0.0` を満たす値だけを構築・受け渡しすること。
pub const EXPECTED_SLIPPAGE_DEDUCTION: f64 = 0.005;

/// Markowitz に渡せる「正常値」を保証するコスト控除比率（return スケール）
///
/// `CostDeduction::new` で `is_finite() && >= 0.0` 不変条件を満たした値のみ構築可能。
/// 上限は業務判定（optimizer 側）に委ねるため設けない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostDeduction(f64);

impl CostDeduction {
    /// `is_finite() && value >= 0.0` を満たす場合のみ `Some` を返す。
    ///
    /// NaN / Infinity / 負値は `None`。これにより `CostDeduction` が
    /// optimizer に渡る時点で NaN cascade の入口を型で塞ぐ。
    pub fn new(value: f64) -> Option<Self> {
        if value.is_finite() && value >= 0.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 内部値を取り出す。
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl From<CostDeduction> for f64 {
    fn from(c: CostDeduction) -> Self {
        c.0
    }
}

/// `to_cost_deduction` の失敗バリアント
///
/// 失敗した token は呼び出し側で `estimation_failures` 経路に合流させ、
/// `retain_tokens` で portfolio から除外することを期待する。
/// `f64::INFINITY` を返して silent に Markowitz に流入させてはならない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CostError {
    /// `assumed_position` が 0 — コスト比率を取引額で割れず計算不能
    ZeroPosition,
    /// derive した比率が `f64::INFINITY` または `f64::NAN`（BigDecimal→f64 変換異常）
    NonFiniteRatio,
}

impl fmt::Display for CostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CostError::ZeroPosition => {
                write!(f, "cost deduction is undefined when assumed_position is 0")
            }
            CostError::NonFiniteRatio => {
                write!(f, "derived cost ratio is non-finite (NaN/Infinity)")
            }
        }
    }
}

impl std::error::Error for CostError {}

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
    /// # 不変条件
    ///
    /// - 戻り値の `CostDeduction` は `is_finite() && >= 0.0` を必ず満たす。
    /// - `assumed_position` が 0 の場合は `Err(CostError::ZeroPosition)`。
    /// - BigDecimal→f64 変換が NaN/Infinity になった場合は
    ///   `Err(CostError::NonFiniteRatio)`。
    ///
    /// # 設計上の注意
    ///
    /// `f64::INFINITY` を返して silent に Markowitz `box_maximize_sharpe` に
    /// 流入させてはならない。Cholesky 後段で `0 × INFINITY = NaN` 連鎖が生じ、
    /// `sum_p.abs() < 1e-15` 等のガード（`common::algorithm::portfolio.rs:739`）
    /// が NaN 比較で防御失効する。失敗 token は `retain_tokens` 経由で
    /// portfolio から除外されるべき。
    pub fn to_cost_deduction(
        &self,
        assumed_position: &YoctoValue,
    ) -> std::result::Result<CostDeduction, CostError> {
        if assumed_position.as_bigdecimal().is_zero() {
            return Err(CostError::ZeroPosition);
        }
        // NEAR スケールで f64 変換 (~10⁻³ オーダー → f64 仮数部範囲内)
        let fixed_near = self.fixed_cost.to_near();
        let position_near = assumed_position.to_near();
        let ratio_bd = fixed_near.as_bigdecimal() / position_near.as_bigdecimal();
        let ratio = ratio_bd.to_f64().ok_or(CostError::NonFiniteRatio)?;
        if !ratio.is_finite() || !self.variable_ratio.is_finite() {
            return Err(CostError::NonFiniteRatio);
        }
        let total = self.variable_ratio + ratio;
        CostDeduction::new(total).ok_or(CostError::NonFiniteRatio)
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
///
/// `storage_min_per_token` および gas yocto 値が `u128` に収まらない場合は
/// `Err` で fail-fast する（silent fallback で `u128::MAX`/`0` を返す挙動は廃止）。
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
    let storage_count = u128::try_from(new_token_count)
        .map_err(|_| anyhow::anyhow!("new_token_count {new_token_count} exceeds u128"))?;
    let storage_per_token = storage_min_per_token
        .as_bigdecimal()
        .to_u128()
        .ok_or_else(|| anyhow::anyhow!("storage_min_per_token does not fit in u128"))?;
    let storage_yocto = storage_per_token.saturating_mul(storage_count);
    let gas_u128 = gas_yocto
        .as_bigdecimal()
        .to_u128()
        .ok_or_else(|| anyhow::anyhow!("gas yocto does not fit in u128"))?;
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
