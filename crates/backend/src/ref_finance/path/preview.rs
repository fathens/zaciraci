use crate::Result;
use crate::ref_finance;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::{TokenPairLike, TokenPath};
use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::types::gas_price::GasPrice;
use near_gas::NearGas;

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
    const HEAD: NearGas = NearGas::from_ggas(2700);
    const BY_STEP: NearGas = NearGas::from_ggas(2600);

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
        let gas = Self::HEAD.as_gas() + Self::BY_STEP.as_gas() * (depth as u64);
        gas as u128 * gas_price.to_balance()
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

impl<M> PreviewList<M>
where
    M: Copy,
{
    pub fn convert<F, D>(&self, converter: F) -> PreviewList<D>
    where
        F: Fn(M) -> D,
    {
        let list = self
            .list
            .iter()
            .map(|p| Preview {
                gas_price: p.gas_price,
                input_value: converter(p.input_value),
                token: p.token.clone(),
                depth: p.depth,
                output_value: p.output_value,
                gain: p.gain,
            })
            .collect();
        PreviewList {
            input_value: converter(self.input_value),
            list,
            total_gain: self.total_gain,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::{TokenAccount, TokenOutAccount};
    use crate::types::{MicroNear, MilliNear};
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
