//! 取引コストの事前見積もり
//!
//! Markowitz 最適化に渡す前に expected_return から AMM 手数料・price impact・
//! ガス・storage・スリッページマージンを差し引くための純関数群。

use crate::Result;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
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
/// 取引額をはるかに超える結果として全トークンが `to_cost_deduction_with_basis`
/// 失敗で portfolio から除外される DoS 経路になり得る。
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

/// `CostDeduction` の上限（return スケール）
///
/// 経済的に意味のある「コスト控除比率 = (variable × trade + fixed) / held」の
/// 上限値。`1.0` (= 100%) は legitimate な小規模ポートフォリオで fixed_cost が
/// dominant な rebalance ケース（held_size が gas 数 NEAR と同等オーダー）を
/// 不正拒否してしまうため厳しすぎる。`1e6` 以上は防御として弱く、実害は出ない
/// が typed config bypass DoS surface への遮断線にならない。
///
/// `10.0` (= 1000%) は経済的に意味のある deduction 範囲（小規模 rebalance の
/// fixed-dominant ケースまで許容）と「`target_w = 1e-300` 経路で `held_size`
/// 経由の巨大 finite ratio が `is_finite()` ガードを通過する」DoS surface 遮断
/// を両立する。
const COST_DEDUCTION_SANE_MAX: f64 = 10.0;

/// Markowitz に渡せる「正常値」を保証するコスト控除比率（return スケール）
///
/// `CostDeduction::new` で `is_finite() && 0.0 <= value <= COST_DEDUCTION_SANE_MAX`
/// 不変条件を満たした値のみ構築可能。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CostDeduction(f64);

impl CostDeduction {
    /// `is_finite() && 0.0 <= value <= COST_DEDUCTION_SANE_MAX` を満たす場合のみ
    /// `Some` を返す。
    ///
    /// NaN / Infinity / 負値 / 上限超過は `None`。これにより `CostDeduction` が
    /// optimizer に渡る時点で NaN cascade と numerical instability の入口を型で
    /// 塞ぐ。`held_size` が `target_w = 1e-300` のような subnormal positive 経由で
    /// 巨大 finite (1e+270 等) になり Markowitz Cholesky 後段で数値破綻する経路
    /// (typed config bypass / hostile RPC) も同じ不変条件で遮断する。
    pub(crate) fn new(value: f64) -> Option<Self> {
        if value.is_finite() && (0.0..=COST_DEDUCTION_SANE_MAX).contains(&value) {
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

/// `to_cost_deduction_with_basis` の失敗バリアント
///
/// 失敗した token は呼び出し側で `estimation_failures` 経路に合流させ、
/// `retain_tokens` で portfolio から除外することを期待する。
/// `f64::INFINITY` を返して silent に Markowitz に流入させてはならない。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub(crate) enum CostError {
    /// `held_size` が 0 — コスト比率を保有額で割れず計算不能（target_w = 0 の full exit 等）。
    #[error("cost deduction is undefined when held_size is 0")]
    ZeroPosition,
    /// derive した比率が `f64::INFINITY` または `f64::NAN`（BigDecimal→f64 変換異常）
    #[error("derived cost ratio is non-finite (NaN/Infinity)")]
    NonFiniteRatio,
    /// derive した比率が `COST_DEDUCTION_SANE_MAX` を超過。
    /// typed config bypass / hostile RPC 経由で `held_size = 1e-300 × total_value`
    /// のような subnormal positive 経路から finite な巨大 ratio が出てきた場合に発火。
    #[error("derived cost ratio {value} exceeds sane upper cap {cap}")]
    ExcessiveRatio { value: f64, cap: f64 },
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
    /// Δw ベースのコスト deduction を計算する（Phase 2 形式）。
    ///
    /// - `trade_size`: 実際にスワップする量（`|Δw| × total_value`、yoctoNEAR）。
    ///   `variable_ratio` は `estimate_trade_cost` 内で `assumed_in = trade_size`
    ///   として算出される前提で渡される（呼び出し側が一致を保つ責任を負う）。
    /// - `held_size`: target weight 下で保有する量（`target_w × total_value`、yoctoNEAR）。
    ///   結果の deduction はこの値をベースに正規化され、Markowitz の
    ///   `expected_return - deduction` と単位整合する。
    ///
    /// 数式:
    ///
    /// ```text
    ///   total_cost_in_near = variable_ratio × trade_size_near + fixed_cost_near
    ///   deduction          = total_cost_in_near / held_size_near
    /// ```
    ///
    /// `held_size = 0`（full exit）は `ZeroPosition` で上位に通知し、呼び出し
    /// 側で 0 deduction として扱う設計を採る。これは Markowitz が target_w=0
    /// の銘柄を `r - deduction` の符号で選好しない（weight × return = 0 で
    /// contribution ゼロ）構造的限界に由来し、本メソッドの責務外。Phase 3 で
    /// regularized Markowitz により target_w=0 の exit cost を
    /// `-C(|Δw|)` として objective に組み込む follow-up を予定。
    ///
    /// # 不変条件
    ///
    /// - 戻り値の `CostDeduction` は `is_finite() && >= 0.0` を必ず満たす。
    /// - `held_size = 0` → `Err(CostError::ZeroPosition)`。
    /// - BigDecimal→f64 変換失敗 → `Err(CostError::NonFiniteRatio)`。
    /// - `variable_ratio` が non-finite → `Err(CostError::NonFiniteRatio)`。
    ///
    /// # 設計上の注意
    ///
    /// `f64::INFINITY` を返して silent に Markowitz `box_maximize_sharpe` に
    /// 流入させてはならない。Cholesky 後段で `0 × INFINITY = NaN` 連鎖が生じ、
    /// `sum_p.abs() < 1e-15` 等のガード（`common::algorithm::portfolio.rs:739`）
    /// が NaN 比較で防御失効する。失敗 token は `retain_tokens` 経由で
    /// portfolio から除外されるべき。
    pub(crate) fn to_cost_deduction_with_basis(
        &self,
        trade_size: &YoctoValue,
        held_size: &YoctoValue,
    ) -> std::result::Result<CostDeduction, CostError> {
        if held_size.as_bigdecimal().is_zero() {
            return Err(CostError::ZeroPosition);
        }
        if !self.variable_ratio.is_finite() {
            return Err(CostError::NonFiniteRatio);
        }
        // BigDecimal で精度を保ったまま「変動費 NEAR + 固定費 NEAR」を構築し、
        // 最後に held_size_near で割って ratio を作る。f64 経由の中間誤差を
        // できるだけ排除しつつ、変換失敗は `NonFiniteRatio` で fail-soft。
        let trade_near = trade_size.to_near();
        let held_near = held_size.to_near();
        let fixed_near = self.fixed_cost.to_near();
        let var_ratio_bd = bigdecimal::BigDecimal::from_f64(self.variable_ratio)
            .ok_or(CostError::NonFiniteRatio)?;
        let var_cost_near = trade_near.as_bigdecimal() * var_ratio_bd;
        let total_cost_near = var_cost_near + fixed_near.as_bigdecimal();
        let ratio_bd = total_cost_near / held_near.as_bigdecimal();
        let ratio = ratio_bd.to_f64().ok_or(CostError::NonFiniteRatio)?;
        // `> COST_DEDUCTION_SANE_MAX` の場合と NaN/Infinity/負値の場合を区別して
        // 上位に通知する。前者は hostile RPC や typed config bypass で `held_size`
        // が subnormal 経由の巨大 finite に跳ねる経路の signal で、後者は
        // BigDecimal -> f64 変換の数値病理ケース。
        if !ratio.is_finite() || ratio < 0.0 {
            return Err(CostError::NonFiniteRatio);
        }
        CostDeduction::new(ratio).ok_or(CostError::ExcessiveRatio {
            value: ratio,
            cap: COST_DEDUCTION_SANE_MAX,
        })
    }
}

/// 往復（BUY + SELL）コストの見積もり
///
/// - **variable_ratio**: BUY 側 (wnear → token) と SELL 側 (token → wnear) を
///   独立に AMM fee + price impact 計算し、それぞれ `EXPECTED_SLIPPAGE_DEDUCTION`
///   を加算した上で合算。BUY のみ計上していた旧実装は `path = wnear → token`
///   前提で書かれていたが、`portfolio_cost` の呼び出し側はラウンドトリップ
///   path (wnear → token → wnear) を渡しており、24 decimals 以外の token では
///   `output_smallest` の単位が桁ズレして `compute_loss_ratio` の負値クランプ
///   で実質 `EXPECTED_SLIPPAGE_DEDUCTION` のみに潰れていた（plan §3 CRITICAL #1
///   参照）。今は方向ごとに helper を分けて単位を揃える。
/// - **fixed_cost**: `estimate_swap_gas_cost_yocto(gas_price, buy_depth + sell_depth)`
///   `+ storage_min × new_token_count`。gas は両 swap 実行ぶん、storage は
///   トークン登録の一回限り。
///
/// `trade_size` は wallet 視点での取引額（NEAR yocto 換算）。BUY 側はそのまま
/// 入力、SELL 側は `spot_rate` で token smallest_units に換算してから path に
/// 通す。`trade_size` が 0 の場合は price impact が計測できないため、
/// variable_ratio は両方向ぶんの `EXPECTED_SLIPPAGE_DEDUCTION` のみ。
///
/// `storage_min_per_token` は `STORAGE_MIN_SANE_CAP`（1 NEAR）で min クランプ
/// するため、RPC が異常に巨大な値（例: `u128::MAX`）を返しても overflow による
/// DoS 経路にならない。gas yocto 値が `u128` に収まらない場合は `Err` で
/// fail-fast する。
pub(crate) fn estimate_trade_cost(
    buy_path: &TokenPath,
    sell_path: &TokenPath,
    trade_size: &YoctoValue,
    spot_rate: &ExchangeRate,
    gas_price: GasPrice,
    storage_min_per_token: &YoctoValue,
    new_token_count: usize,
) -> Result<TradeCostBreakdown> {
    // cron tick path で呼ばれるため `debug_assert!` は使わない（CONTRIBUTING.md
    // の cron-path assert ban 趣旨）。caller 側のロジック異常で `cap × N` が
    // 膨れて `fixed_cost` が取引額を超え、全 token が `to_cost_deduction_with_basis`
    // 失敗で除外される DoS 経路を runtime で遮断する。
    if new_token_count > MAX_NEW_TOKEN_COUNT {
        anyhow::bail!(
            "new_token_count {new_token_count} exceeds MAX_NEW_TOKEN_COUNT {MAX_NEW_TOKEN_COUNT}"
        );
    }

    let buy_variable = compute_buy_variable_ratio(buy_path, trade_size, spot_rate)?;
    let sell_variable = compute_sell_variable_ratio(sell_path, trade_size, spot_rate)?;
    let variable_ratio = buy_variable + sell_variable;

    let depth = buy_path.len() + sell_path.len();
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

/// Markowitz に渡せる「正常値」を保証する往復コスト比率（return スケール）
///
/// 入力 `trade_size` に対する `(variable × trade + fixed) / trade_size` を
/// `[0.0, COST_DEDUCTION_SANE_MAX]` の正常範囲で保持する。alpha gate で
/// `H × ER > k × round_trip_cost` 比較に使う。
///
/// `CostDeduction` (= cost / held_size) と異なり basis が `trade_size` 自身で
/// あること、および NaN/Infinity 排除の不変条件が型として明示されることが
/// 価値。alpha gate ヘルパは `Result<RoundTripCostRatio>` で失敗を上位に
/// 透過させ、`f64::INFINITY` を Markowitz 周辺に流入させない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoundTripCostRatio(f64);

impl RoundTripCostRatio {
    /// `is_finite() && 0.0 <= value <= COST_DEDUCTION_SANE_MAX` を満たす場合のみ
    /// `Some` を返す。`CostDeduction::new` と同じ不変条件で、alpha gate と
    /// cost-aware deduction の return スケール基準を統一する。
    pub(crate) fn new(value: f64) -> Option<Self> {
        if value.is_finite() && (0.0..=COST_DEDUCTION_SANE_MAX).contains(&value) {
            Some(Self(value))
        } else {
            None
        }
    }

    pub(crate) fn as_f64(self) -> f64 {
        self.0
    }
}

impl From<RoundTripCostRatio> for f64 {
    fn from(c: RoundTripCostRatio) -> Self {
        c.0
    }
}

/// フルポジション size を 1 度買って 1 度売る往復コスト比率を見積もる
///
/// alpha gate (`trade::alpha_gate`) からの呼び出し専用。Markowitz の
/// per-period `expected_return` (= ratio) と単位整合する `RoundTripCostRatio`
/// を返す。
///
/// 計算式:
///
/// ```text
///   breakdown = estimate_trade_cost(buy_path, sell_path, position_size, ...)
///   ratio     = breakdown.variable_ratio + breakdown.fixed_cost_near / position_size_near
/// ```
///
/// `position_size = 0` は割り算が定義されないため `CostError::ZeroPosition`
/// と同等の `Err` で fail-soft する（呼び出し側は当該 token を除外する想定）。
pub(crate) fn estimate_full_position_round_trip_ratio(
    buy_path: &TokenPath,
    sell_path: &TokenPath,
    position_size: &YoctoValue,
    spot_rate: &ExchangeRate,
    gas_price: GasPrice,
    storage_min_per_token: &YoctoValue,
    new_token_count: usize,
) -> Result<RoundTripCostRatio> {
    if position_size.as_bigdecimal().is_zero() {
        anyhow::bail!("round-trip cost is undefined when position_size is 0");
    }
    let breakdown = estimate_trade_cost(
        buy_path,
        sell_path,
        position_size,
        spot_rate,
        gas_price,
        storage_min_per_token,
        new_token_count,
    )?;
    if !breakdown.variable_ratio.is_finite() {
        anyhow::bail!(
            "variable_ratio is non-finite: {:?}",
            breakdown.variable_ratio
        );
    }
    let position_near = position_size.to_near();
    let fixed_near = breakdown.fixed_cost.to_near();
    let fixed_per_trade = fixed_near.as_bigdecimal() / position_near.as_bigdecimal();
    let fixed_f64 = fixed_per_trade.to_f64().ok_or_else(|| {
        anyhow::anyhow!("fixed_cost / position_size does not fit in f64 (non-finite ratio)")
    })?;
    let ratio = breakdown.variable_ratio + fixed_f64;
    RoundTripCostRatio::new(ratio).ok_or_else(|| {
        anyhow::anyhow!(
            "round-trip ratio {} outside sane range [0, {}]",
            ratio,
            COST_DEDUCTION_SANE_MAX
        )
    })
}

/// `storage_min_per_token` を `STORAGE_MIN_SANE_CAP` で min クランプして u128 化する。
///
/// RPC 由来の値が `u128` に収まらない or 上限を超える場合は `STORAGE_MIN_SANE_CAP`
/// にクランプされる。これにより `saturating_mul(new_token_count)` が
/// `u128::MAX` に張り付いて固定コストが取引額を超え、全トークンが
/// `to_cost_deduction_with_basis` 失敗で portfolio から除外される DoS 経路を遮断する。
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

/// BUY 方向 (wnear → token) の variable_ratio
///
/// `trade_size` は NEAR yocto 入力、`buy_path` は wnear を入口とするマルチ
/// ホップ列。`path.calc_value(trade_size_yocto)` の出力は target token の
/// smallest_units、それを `spot_rate` で割って NEAR 換算し
/// `(input - output) / input` を計算する。`EXPECTED_SLIPPAGE_DEDUCTION` を
/// 加えて返す。0 入力は impact 不可測のため `EXPECTED_SLIPPAGE_DEDUCTION` のみ。
fn compute_buy_variable_ratio(
    buy_path: &TokenPath,
    trade_size: &YoctoValue,
    spot_rate: &ExchangeRate,
) -> Result<f64> {
    let input_yocto = trade_size.as_bigdecimal().to_u128().ok_or_else(|| {
        anyhow::anyhow!(
            "buy trade_size too large to convert to u128: trade_yocto={:.6e}",
            trade_size.as_bigdecimal().to_f64().unwrap_or(f64::NAN)
        )
    })?;
    if input_yocto == 0 {
        return Ok(EXPECTED_SLIPPAGE_DEDUCTION);
    }

    let output_token_smallest = buy_path.calc_value(input_yocto)?;
    let output_amount = TokenAmount::from_smallest_units(
        BigDecimal::from(output_token_smallest),
        spot_rate.decimals(),
    );
    let output_near = (&output_amount) / spot_rate;
    let input_near = trade_size.to_near();

    let amm_loss = compute_loss_ratio(&input_near, &output_near)?;
    Ok(amm_loss + EXPECTED_SLIPPAGE_DEDUCTION)
}

/// SELL 方向 (token → wnear) の variable_ratio
///
/// `trade_size` は NEAR yocto で表した取引額。BUY と単位を揃えるため、ここで
/// `spot_rate` を使って token smallest_units に換算してから `sell_path` に
/// 通す。出力は wnear 側 (= NEAR yocto) なのでそのまま NEAR に戻す。
/// `EXPECTED_SLIPPAGE_DEDUCTION` を加えて返す。
///
/// `trade_size` が 0、または spot_rate 換算後に 0 smallest_units になる
/// （極端に高価値・低 decimals なトークンの極小取引）場合は impact 不可測
/// として `EXPECTED_SLIPPAGE_DEDUCTION` のみ返す。
fn compute_sell_variable_ratio(
    sell_path: &TokenPath,
    trade_size: &YoctoValue,
    spot_rate: &ExchangeRate,
) -> Result<f64> {
    let input_yocto = trade_size.as_bigdecimal().to_u128().ok_or_else(|| {
        anyhow::anyhow!(
            "sell trade_size too large to convert to u128: trade_yocto={:.6e}",
            trade_size.as_bigdecimal().to_f64().unwrap_or(f64::NAN)
        )
    })?;
    if input_yocto == 0 {
        return Ok(EXPECTED_SLIPPAGE_DEDUCTION);
    }
    if spot_rate.is_effectively_zero() {
        // raw_rate < 1 は 1 NEAR で 1 smallest_unit 未満 = 取引不能 token。
        // SELL impact は計算不能なので buffer のみ。
        return Ok(EXPECTED_SLIPPAGE_DEDUCTION);
    }

    // token_smallest = trade_size_NEAR × spot_rate.raw_rate
    let input_near_bd = trade_size.to_near().as_bigdecimal().clone();
    let input_token_bd = &input_near_bd * spot_rate.raw_rate();
    let Some(input_token_smallest) = input_token_bd.to_u128() else {
        // BigDecimal の Display は重いため、f64 round-trip でログ flood を防ぐ。
        // 値を埋めることで「raw_rate × trade_size が u128 を溢れる token」が
        // ログ・診断で識別可能になる（高 decimals memecoin、敵対 RPC の異常
        // pool 等）。
        anyhow::bail!(
            "sell input token smallest_units does not fit in u128: \
             input_near={:.6e}, raw_rate={:.6e}",
            input_near_bd.to_f64().unwrap_or(f64::NAN),
            spot_rate.raw_rate().to_f64().unwrap_or(f64::NAN),
        );
    };
    if input_token_smallest == 0 {
        return Ok(EXPECTED_SLIPPAGE_DEDUCTION);
    }

    let output_yocto = sell_path.calc_value(input_token_smallest)?;
    let output_near = YoctoValue::from_yocto_u128(output_yocto).to_near();
    let input_near = trade_size.to_near();

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
/// caller (`compute_variable_ratio` → `to_cost_deduction_with_basis` → `compute_cost_deductions`)
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
