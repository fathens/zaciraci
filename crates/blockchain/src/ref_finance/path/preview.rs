use crate::Result;
use crate::ref_finance;
use crate::ref_finance::path::graph::TokenGraph;
use crate::types::gas_price::GasPrice;
use common::types::{TokenAccount, YoctoValue};
use common::types::{TokenInAccount, TokenOutAccount};
use dex::{TokenPairLike, TokenPath};
use logging::*;
use near_gas::NearGas;

/// swap の固定ガス（パス先頭の関数呼び出し）
const HEAD_GAS: NearGas = NearGas::from_ggas(2700);
/// swap の per-hop ガス
const BY_STEP_GAS: NearGas = NearGas::from_ggas(2600);

/// `swap_gas_cost_yocto_u128` の sanity cap（= 1 NEAR）。
///
/// production の swap gas は `(HEAD + BY_STEP × depth) × gas_price ≈ 数百 microNEAR`
/// オーダーで、1 NEAR は production baseline の約 1000× に相当する。`saturating_mul`
/// による u128 オーバーフロー防御に加え、敵対 RPC が異常な `gas_price` を返した
/// 場合の attack surface（cost-aware optimization で全 token に巨大 cost_deduction
/// が適用され equal-weight rebalance を強制される攻撃経路）を構造的に縮小する。
///
/// この cap は SSoT [`swap_gas_cost_yocto_u128`] 内で適用されるため、`Preview::cost`
/// （arbitrage 経路）と `estimate_swap_gas_cost_yocto`（trade 経路）の両方が同時に
/// 防御される。STORAGE_MIN_SANE_CAP と対称的な「sanity cap」パターン。
const GAS_YOCTO_SANE_CAP: u128 = 10u128.pow(24);

/// 指定 depth の swap で消費するガス料金を yoctoNEAR を u128 で算出する SSoT。
///
/// `(HEAD + BY_STEP * depth) * gas_price` を saturating 算術で計算し、
/// 結果を [`GAS_YOCTO_SANE_CAP`] にクランプする。公開 API の
/// [`estimate_swap_gas_cost_yocto`] と private な [`Preview::cost`] が共に
/// この関数を経由するため、計算式と cap 適用は完全に一致する。
///
/// cap が発動した場合は `warn!` を `std::sync::Once` で一度だけ emit し、
/// 攻撃検知シグナルを残しつつホットパス連発を防ぐ。
fn swap_gas_cost_yocto_u128(gas_price: GasPrice, depth: usize) -> u128 {
    let gas = HEAD_GAS
        .as_gas()
        .saturating_add(BY_STEP_GAS.as_gas().saturating_mul(depth as u64));
    let raw = (gas as u128).saturating_mul(gas_price.to_balance());
    if raw > GAS_YOCTO_SANE_CAP {
        static CAP_WARN: std::sync::Once = std::sync::Once::new();
        CAP_WARN.call_once(|| {
            let log = DEFAULT.new(o!("function" => "swap_gas_cost_yocto_u128"));
            warn!(log, "gas yocto clamped to sane cap";
                "raw" => raw,
                "cap" => GAS_YOCTO_SANE_CAP,
                "depth" => depth,
                "gas_price_yocto" => gas_price.to_balance(),
            );
        });
    }
    raw.min(GAS_YOCTO_SANE_CAP)
}

/// 指定 depth の swap で消費するガス料金を yoctoNEAR で見積もる。
///
/// 内部で [`swap_gas_cost_yocto_u128`] を呼ぶ薄いラッパで、`YoctoValue` を返す。
/// 外部クレート（trade 等）からもコスト推定できるよう公開する。
pub fn estimate_swap_gas_cost_yocto(gas_price: GasPrice, depth: usize) -> YoctoValue {
    YoctoValue::from_yocto_u128(swap_gas_cost_yocto_u128(gas_price, depth))
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Preview<M> {
    pub gas_price: GasPrice,
    pub input_value: M,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
    pub gain: u128,
}

impl<M> Preview<M>
where
    M: Into<u128> + Copy,
{
    pub fn new(
        gas_price: GasPrice,
        input_value: M,
        token: TokenOutAccount,
        depth: usize,
        output_value: u128,
    ) -> Self {
        let gain = Self::gain(gas_price, depth, input_value, output_value);
        Preview {
            gas_price,
            input_value,
            token,
            depth,
            output_value,
            gain,
        }
    }

    fn cost(gas_price: GasPrice, depth: usize) -> u128 {
        swap_gas_cost_yocto_u128(gas_price, depth)
    }

    fn gain(gas_price: GasPrice, depth: usize, input_value: M, output_value: u128) -> u128 {
        let input_value = input_value.into();
        if output_value <= input_value {
            return 0;
        }
        let gain = output_value - input_value;
        let cost = Self::cost(gas_price, depth);
        if gain <= cost {
            return 0;
        }
        gain - cost
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct PreviewList<M> {
    pub input_value: M,
    pub list: Vec<Preview<M>>,
    pub total_gain: u128,
}

impl<M> PreviewList<M> {
    pub fn new(input_value: M, previews: Vec<Preview<M>>) -> Option<Self> {
        let total_gain: u128 = previews.iter().map(|p| p.gain).sum();
        Some(PreviewList {
            input_value,
            list: previews,
            total_gain,
        })
    }

    pub async fn into_with_path(
        self,
        graph: &TokenGraph,
        start: &TokenInAccount,
    ) -> Result<(Vec<(Preview<M>, TokenPath)>, Vec<TokenAccount>)> {
        let mut tokens = Vec::new();
        let mut pre_path = Vec::new();
        for p in self.list {
            let path = ref_finance::path::swap_path(graph, start, &p.token).await?;
            for pair in path.0.iter() {
                tokens.push(pair.token_in_id().into());
                tokens.push(pair.token_out_id().into());
            }
            pre_path.push((p, path));
        }
        tokens.sort();
        tokens.dedup();

        Ok((pre_path, tokens))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MicroNear, MilliNear};
    use common::types::TokenAccount;
    use common::types::TokenOutAccount;
    use near_sdk::NearToken;

    fn token_out(token: &str) -> TokenOutAccount {
        let token: TokenAccount = token.parse().unwrap();
        token.into()
    }

    const HEAD: u128 = MicroNear::of(270).to_yocto();
    const BY_STEP: u128 = MicroNear::of(260).to_yocto();
    const MIN_GAS_PRICE: GasPrice = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));

    #[test]
    fn test_preview_cost() {
        assert_eq!(Preview::<MilliNear>::cost(MIN_GAS_PRICE, 1), HEAD + BY_STEP);
        assert_eq!(
            Preview::<MilliNear>::cost(MIN_GAS_PRICE, 2),
            HEAD + 2 * BY_STEP
        );
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_matches_estimate() {
        // SSoT 関数と公開ラッパが同一値を返すことを保証する。
        for depth in [0usize, 1, 2, 3, 5] {
            let direct = swap_gas_cost_yocto_u128(MIN_GAS_PRICE, depth);
            let via_yocto = estimate_swap_gas_cost_yocto(MIN_GAS_PRICE, depth);
            assert_eq!(YoctoValue::from_yocto_u128(direct), via_yocto);
        }
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_zero_depth() {
        // depth=0 の場合は HEAD のみ。
        assert_eq!(swap_gas_cost_yocto_u128(MIN_GAS_PRICE, 0), HEAD);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_below_cap_passthrough() {
        // production gas_price は ~10^8 yocto/gas、cap (10^24) には 7 桁の余裕がある。
        // 通常の depth ではクランプは発動せず、入力どおりの値が返る。
        let result = swap_gas_cost_yocto_u128(MIN_GAS_PRICE, 5);
        assert!(result < GAS_YOCTO_SANE_CAP);
        let expected = HEAD + 5 * BY_STEP;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_above_cap_clamped() {
        // 異常な gas_price（敵対 RPC を模した値）を渡すと cap でクランプされる。
        // 1 NEAR / (HEAD + BY_STEP) ≈ 10^24 / 5300 ggas ≈ 1.9e14 が境界、
        // それを超える gas_price では確実にクランプ発動。
        let hostile = GasPrice::from_balance(NearToken::from_yoctonear(10u128.pow(20)));
        let result = swap_gas_cost_yocto_u128(hostile, 1);
        assert_eq!(result, GAS_YOCTO_SANE_CAP);
    }

    #[test]
    fn test_swap_gas_cost_yocto_u128_at_u64_max_gas_price_clamped() {
        // gas_price が u64::MAX に張り付いた場合（saturating_mul 経由の overflow path）
        // も cap でクランプされ u128::MAX に発散しない。
        let max_gas_price = GasPrice::from_balance(NearToken::from_yoctonear(u64::MAX as u128));
        let result = swap_gas_cost_yocto_u128(max_gas_price, 5);
        assert_eq!(result, GAS_YOCTO_SANE_CAP);
    }

    #[test]
    fn test_preview_gain() {
        assert_eq!(
            Preview::gain(
                MIN_GAS_PRICE,
                1,
                MilliNear::of(100),
                MilliNear::of(300).to_yocto()
            ),
            MilliNear::of(200).to_yocto() - HEAD - BY_STEP
        );

        assert_eq!(
            Preview::gain(
                MIN_GAS_PRICE,
                2,
                MicroNear::of(100_000),
                MilliNear::of(200).to_yocto()
            ),
            MilliNear::of(100).to_yocto() - HEAD - 2 * BY_STEP
        );
    }

    #[test]
    fn test_preview_list_total_gain_milli() {
        let a = Preview::new(
            MIN_GAS_PRICE,
            MilliNear::of(100),
            token_out("a.token"),
            1,
            MilliNear::of(300).to_yocto(),
        );
        let b = Preview::new(
            MIN_GAS_PRICE,
            MilliNear::of(100),
            token_out("b.token"),
            1,
            MilliNear::of(200).to_yocto(),
        );
        let previews = vec![a.clone(), b.clone()];
        let preview_list = PreviewList::new(MilliNear::of(100), previews).unwrap();
        assert_eq!(preview_list.total_gain, a.gain + b.gain);
    }

    #[test]
    fn test_preview_list_total_gain_micro() {
        let a = Preview::new(
            MIN_GAS_PRICE,
            MicroNear::of(100_000),
            token_out("a.token"),
            1,
            MilliNear::of(300).to_yocto(),
        );
        let b = Preview::new(
            MIN_GAS_PRICE,
            MicroNear::of(100_000),
            token_out("b.token"),
            1,
            MilliNear::of(200).to_yocto(),
        );
        let previews = vec![a.clone(), b.clone()];
        let preview_list = PreviewList::new(MicroNear::of(100_000), previews).unwrap();
        assert_eq!(preview_list.total_gain, a.gain + b.gain);
    }
}
