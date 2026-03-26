use crate::Result;

/// estimate と実行価格の乖離を吸収する最低限の余裕（50 BPS = 0.5%）
///
/// estimate_return は AMM 手数料を織り込み済みのため、この値は
/// DB データ鮮度遅延・他トレーダーによるプール状態変化・ブロック間価格変動をカバーする。
const MIN_SLIPPAGE_BUDGET: f64 = 0.005;

/// 高ボラティリティでも元本の 85% は保護（1500 BPS = 15%）
const MAX_SLIPPAGE_BUDGET: f64 = 0.15;

/// スリッページ保護の方針
///
/// トレードを決断した予測リターンそのものがスリッページ予算となる。
/// 設定値による任意の%指定ではなく、取引判断の根拠を型で明示する。
#[derive(Debug, Clone)]
pub enum SlippagePolicy {
    /// 予測リターンに基づくスリッページ保護
    ///
    /// `min_out = estimated_output * (1 - slippage_budget)` を適用する。
    /// slippage_budget は expected_return を MIN/MAX でクランプした値。
    FromExpectedReturn(ExpectedReturn),

    /// スリッページ保護なし（min_out = 0）
    ///
    /// 清算や売却フェーズなど、確実な約定を優先する場合に明示的に選択する。
    Unprotected,
}

/// 予測モデルが示す期待リターン（比率）
///
/// 例: 0.05 = 5% の上昇予測
#[derive(Debug, Clone, Copy)]
pub struct ExpectedReturn(f64);

impl ExpectedReturn {
    pub fn new(ratio: f64) -> Self {
        Self(ratio)
    }

    pub fn as_ratio(&self) -> f64 {
        self.0
    }
}

/// AMM の理論出力とスリッページポリシーから min_out を計算する（BPS 整数演算）
///
/// - `FromExpectedReturn`: expected_return の絶対値を MIN/MAX でクランプし、
///   BPS に変換して整数演算で min_out を算出
/// - `Unprotected`: min_out = 0 を返す
///
/// # Errors
///
/// `estimated_output * protection_bps` が u128 をオーバーフローした場合にエラーを返す。
/// min_out: 0 へのフォールバックは行わない（スリッページ保護の無効化を防止）。
pub fn calculate_min_out(estimated_output: u128, policy: &SlippagePolicy) -> Result<u128> {
    let SlippagePolicy::FromExpectedReturn(expected) = policy else {
        return Ok(0);
    };
    let budget = expected
        .as_ratio()
        .abs()
        .clamp(MIN_SLIPPAGE_BUDGET, MAX_SLIPPAGE_BUDGET);
    let slippage_bps = (budget * 10_000.0) as u128;
    let protection_bps = 10_000u128.saturating_sub(slippage_bps);
    estimated_output
        .checked_mul(protection_bps)
        .map(|v| v / 10_000)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "min_out overflow: estimated={}, protection_bps={}",
                estimated_output,
                protection_bps
            )
        })
}

impl std::fmt::Display for SlippagePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SlippagePolicy::FromExpectedReturn(er) => {
                write!(f, "FromExpectedReturn({:.4})", er.as_ratio())
            }
            SlippagePolicy::Unprotected => write!(f, "Unprotected"),
        }
    }
}

#[cfg(test)]
mod tests;
