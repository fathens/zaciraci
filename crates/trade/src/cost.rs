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
use logging::*;

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
pub(crate) const EXPECTED_SLIPPAGE_DEDUCTION: f64 = 0.005;

/// `storage_min_per_token` の sanity cap（yoctoNEAR）
///
/// RPC 由来の `storage_min` が壊れたノード／敵対的応答で異常に巨大な値
/// （例: `u128::MAX`）を返した場合、`storage_per_token × new_token_count`
/// が overflow して `saturating_mul` で `u128::MAX` に張り付き、固定コストが
/// 取引額をはるかに超える結果として全トークンが `to_cost_deduction` 失敗で
/// portfolio から除外される DoS 経路になり得る。
///
/// 実運用上の `storage_min` は 10⁻⁴ NEAR ～ 1 NEAR オーダーなので、
/// 1 NEAR を上限として min クランプし、境界で異常値を遮断する。
/// 中ポートフォリオ（おおよそ 165–1515 NEAR レンジ）の hostile RPC 全 token
/// 脱落 DoS 経路を保護する。`GAS_YOCTO_SANE_CAP`（100 mNEAR、production
/// baseline の 370×）と「実運用 10× オーダー」基準で対称。
const STORAGE_MIN_SANE_CAP: u128 = 10u128.pow(24);

/// `new_token_count` の debug_assert 上限
///
/// `portfolio_cost::compute_cost_deductions` 経路では token 1 件あたり
/// 0 または 1 しか渡されない。`MAX_HOLDINGS = 6` を踏まえても 16 は
/// 十分なマージンであり、これを超える場合は呼び出し側のロジック異常を示す。
const MAX_NEW_TOKEN_COUNT: usize = 16;

/// `compute_loss_ratio` が負値を「警告に値する」とみなす絶対値閾値
///
/// `(input - output) / input < -LOSS_RATIO_NEGATIVE_WARN_THRESHOLD` の場合、
/// 浮動小数点ノイズでは説明しきれない大きさの「output > input」が観測されており、
/// `spot_rate` と AMM 状態の inconsistency シグナルとして `warn!` を残す。
/// それ以外の負値（絶対値が閾値以下のもの — `1e-9` 級の f64 変換ノイズから
/// `-1e-4` 級のグレーゾーンまで）は数値誤差／AMM 内部丸めとして silent に 0 へ
/// クランプし、運用ノイズを増やさない。
const LOSS_RATIO_NEGATIVE_WARN_THRESHOLD: f64 = 1e-3;

/// Markowitz に渡せる「正常値」を保証するコスト控除比率（return スケール）
///
/// `CostDeduction::new` で `is_finite() && >= 0.0` 不変条件を満たした値のみ構築可能。
/// 上限は業務判定（optimizer 側）に委ねるため設けない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CostDeduction(f64);

impl CostDeduction {
    /// `is_finite() && value >= 0.0` を満たす場合のみ `Some` を返す。
    ///
    /// NaN / Infinity / 負値は `None`。これにより `CostDeduction` が
    /// optimizer に渡る時点で NaN cascade の入口を型で塞ぐ。
    pub(crate) fn new(value: f64) -> Option<Self> {
        if value.is_finite() && value >= 0.0 {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 内部値を取り出す。
    pub(crate) fn as_f64(self) -> f64 {
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CostError {
    /// `assumed_position` が 0 — コスト比率を取引額で割れず計算不能
    #[error("cost deduction is undefined when assumed_position is 0")]
    ZeroPosition,
    /// derive した比率が `f64::INFINITY` または `f64::NAN`（BigDecimal→f64 変換異常）
    #[error("derived cost ratio is non-finite (NaN/Infinity)")]
    NonFiniteRatio,
}

/// 取引コストの内訳
///
/// - `variable_ratio`: AMM fee + price impact + slippage（取引額に比例しない比率部分）
/// - `fixed_cost`: gas + storage（取引したら掛かる固定費、yoctoNEAR 単位）
#[derive(Debug, Clone)]
pub(crate) struct TradeCostBreakdown {
    variable_ratio: f64,
    fixed_cost: YoctoValue,
}

impl TradeCostBreakdown {
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
    pub(crate) fn to_cost_deduction(
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
        // `BigDecimal::to_f64` は値が大きすぎて f64 に収まらない場合に `None` を
        // 返す。`?` で `None` 経路は `NonFiniteRatio` として伝播するため、以降の
        // `ratio` は有限性が保証されており再検査は不要。
        let ratio = ratio_bd.to_f64().ok_or(CostError::NonFiniteRatio)?;
        // `variable_ratio` は self の別経路（`compute_variable_ratio`）から来る
        // 独立なフィールドのため、`ratio` の有限性とは別ラインで検査する。
        if !self.variable_ratio.is_finite() {
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
/// `storage_min_per_token` は `STORAGE_MIN_SANE_CAP`（1 NEAR）で min クランプ
/// するため、RPC が異常に巨大な値（例: `u128::MAX`）を返しても overflow による
/// DoS 経路にならない。gas yocto 値が `u128` に収まらない場合は `Err` で
/// fail-fast する。
pub(crate) fn estimate_trade_cost(
    path: &TokenPath,
    assumed_in: &YoctoValue,
    spot_rate: &ExchangeRate,
    gas_price: GasPrice,
    storage_min_per_token: &YoctoValue,
    new_token_count: usize,
) -> Result<TradeCostBreakdown> {
    // cron tick path で呼ばれるため `debug_assert!` は使わない（CONTRIBUTING.md
    // の cron-path assert ban 趣旨）。caller 側のロジック異常で `cap × N` が
    // 膨れて `fixed_cost` が取引額を超え、全 token が `to_cost_deduction` 失敗
    // で除外される DoS 経路を runtime で遮断する。
    if new_token_count > MAX_NEW_TOKEN_COUNT {
        anyhow::bail!(
            "new_token_count {new_token_count} exceeds MAX_NEW_TOKEN_COUNT {MAX_NEW_TOKEN_COUNT}"
        );
    }

    let depth = path.len();

    let variable_ratio = compute_variable_ratio(path, assumed_in, spot_rate)?;

    let gas_yocto = estimate_swap_gas_cost_yocto(gas_price, depth);
    let storage_count = u128::try_from(new_token_count)
        .map_err(|_| anyhow::anyhow!("new_token_count {new_token_count} exceeds u128"))?;
    let storage_per_token = clamp_storage_min(storage_min_per_token);
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

/// `storage_min_per_token` を `STORAGE_MIN_SANE_CAP` で min クランプして u128 化する。
///
/// RPC 由来の値が `u128` に収まらない or 上限を超える場合は `STORAGE_MIN_SANE_CAP`
/// にクランプされる。これにより `saturating_mul(new_token_count)` が
/// `u128::MAX` に張り付いて固定コストが取引額を超え、全トークンが
/// `to_cost_deduction` 失敗で portfolio から除外される DoS 経路を遮断する。
///
/// # observability
///
/// クランプが発動した場合は `warn!` ログを出す。実運用では `storage_min` が
/// 1 NEAR を超えること自体が異常（悪意ノード接続 / contract migration バグ /
/// node 破損のシグナル）なので、サイレントに吸収せず痕跡を残す。
fn clamp_storage_min(storage_min_per_token: &YoctoValue) -> u128 {
    let raw = storage_min_per_token
        .as_bigdecimal()
        .to_u128()
        .unwrap_or(u128::MAX);
    if raw > STORAGE_MIN_SANE_CAP {
        let log = DEFAULT.new(o!("function" => "clamp_storage_min"));
        warn!(
            log,
            "storage_min clamped to sane cap";
            "raw" => raw,
            "cap" => STORAGE_MIN_SANE_CAP,
        );
    }
    raw.min(STORAGE_MIN_SANE_CAP)
}

/// `assumed_in` を path に通したときの実効的な loss ratio
///
/// `(input_NEAR - output_NEAR_via_spot_rate) / input_NEAR` で AMM fee と price
/// impact を一括計算し、`EXPECTED_SLIPPAGE_DEDUCTION` を加算して返す。
///
/// # モデル上の仮定
///
/// **Entry 片道のみを計上**する。rebalance で生じる exit 側の swap コストは
/// この値には含まれない。これは「rebalance 周期 >> 予測 horizon」を仮定
/// した近似であり、保有期間中に予測リターンで exit コストを十分回収できる
/// 前提に立つ。短期回転の戦略では往復コストへの拡張が必要だが、現行の
/// trade ループはこの前提下で運用されている。
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

    let amm_loss = compute_loss_ratio(&input_near, &output_near)?;
    Ok(amm_loss + EXPECTED_SLIPPAGE_DEDUCTION)
}

/// `(input - output) / input` を非負クランプして f64 で返す
///
/// 数値誤差で出力が入力をわずかに上回る（負の loss）ケースは 0.0 にクランプ。
///
/// # observability
///
/// 負値が `-LOSS_RATIO_NEGATIVE_WARN_THRESHOLD`（= -1e-3）を下回る場合は
/// 浮動小数点ノイズでは説明できない規模の「output > input」が観測されており、
/// `spot_rate` と AMM 状態の inconsistency シグナルとして `warn!` ログを残す。
/// 返り値自体はサイレント時と同じく 0.0 にクランプする（caller への挙動互換）。
///
/// # f64 変換失敗時の挙動
///
/// `BigDecimal::to_f64` は bigdecimal クレート仕様上ほぼ `None` を返さないが、
/// 極端な scale を持つ BigDecimal で失敗する余地は残る。失敗時は `Err` を伝播し、
/// caller (`compute_variable_ratio` → `to_cost_deduction` → `compute_cost_deductions`)
/// で `estimation_failures` 経路に合流させて当該 token を portfolio から除外する。
/// 旧実装は `EXPECTED_SLIPPAGE_DEDUCTION` を返していたが、caller 側で同定数を再加算する
/// ため二重加算（実質 1.0%）になる semantic mistake と、敵対 RPC 経由で
/// f64 変換失敗を induce した際に「実損失 5-50% を 0.5% と過小見積りする」DoS
/// 経路の両方を抱えていた。`Result` 化で既存 4 層 fail-soft 経路に統合する。
fn compute_loss_ratio(input_near: &NearValue, output_near: &NearValue) -> Result<f64> {
    let input_bd = input_near.as_bigdecimal();
    if input_bd <= &BigDecimal::zero() {
        return Ok(0.0);
    }
    let output_bd = output_near.as_bigdecimal();
    let raw = ((input_bd - output_bd) / input_bd)
        .to_f64()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "BigDecimal -> f64 conversion failed; input={input_bd}, output={output_bd}"
            )
        })?;
    if raw < -LOSS_RATIO_NEGATIVE_WARN_THRESHOLD {
        let log = DEFAULT.new(o!("function" => "compute_loss_ratio"));
        warn!(
            log,
            "loss ratio significantly negative; spot_rate / AMM state inconsistency suspected";
            "raw" => raw,
            "threshold" => -LOSS_RATIO_NEGATIVE_WARN_THRESHOLD,
            "input_near" => %input_bd,
            "output_near" => %output_bd,
        );
    }
    Ok(raw.max(0.0))
}

#[cfg(test)]
mod tests;
