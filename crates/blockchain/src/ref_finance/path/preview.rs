use crate::Result;
use crate::ref_finance;
use crate::ref_finance::path::graph::TokenGraph;
use crate::types::gas_price::GasPrice;
use common::types::{TokenAccount, YoctoValue};
use common::types::{TokenInAccount, TokenOutAccount};
use dex::{TokenPairLike, TokenPath};
use near_gas::NearGas;

/// swap の固定ガス（パス先頭の関数呼び出し）
const HEAD_GAS: NearGas = NearGas::from_ggas(2700);
/// swap の per-hop ガス
const BY_STEP_GAS: NearGas = NearGas::from_ggas(2600);

/// 指定 depth の swap で消費するガス料金を yoctoNEAR で見積もる。
///
/// `Preview::cost` と同じ計算式（`(HEAD + BY_STEP * depth) * gas_price`）で、
/// 外部クレート（trade 等）からもコスト推定できるよう公開する Single Source of Truth。
pub fn estimate_swap_gas_cost_yocto(gas_price: GasPrice, depth: usize) -> YoctoValue {
    let gas = HEAD_GAS.as_gas() + BY_STEP_GAS.as_gas() * (depth as u64);
    let yocto = (gas as u128).saturating_mul(gas_price.to_balance());
    YoctoValue::from_yocto_u128(yocto)
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
        let gas = HEAD_GAS.as_gas() + BY_STEP_GAS.as_gas() * (depth as u64);
        (gas as u128).saturating_mul(gas_price.to_balance())
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
    fn test_estimate_swap_gas_cost_yocto_matches_preview_cost() {
        use num_traits::ToPrimitive;
        // 公開 API と private cost が同じ計算式（Single Source of Truth）であることを保証
        for depth in 0..=3 {
            let exported = estimate_swap_gas_cost_yocto(MIN_GAS_PRICE, depth);
            let exported_u128 = exported.as_bigdecimal().to_u128().unwrap();
            let internal = Preview::<MilliNear>::cost(MIN_GAS_PRICE, depth);
            assert_eq!(exported_u128, internal, "mismatch at depth={depth}");
        }
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
