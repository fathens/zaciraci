use crate::milli_near::MilliNear;
use crate::ref_finance::token_account::TokenOutAccount;
use near_gas::NearGas;
use near_primitives::types::Balance;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Preview {
    pub gas_price: Balance,
    pub input_value: MilliNear,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
    pub gain: Balance,
}

impl Preview {
    const HEAD: NearGas = NearGas::from_ggas(2700);
    const BY_STEP: NearGas = NearGas::from_ggas(2600);

    pub fn new(
        gas_price: Balance,
        input_value: MilliNear,
        token: TokenOutAccount,
        depth: usize,
        output_value: Balance,
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

    fn cost(gas_price: Balance, depth: usize) -> u128 {
        let gas = Self::HEAD.as_gas() + Self::BY_STEP.as_gas() * (depth as u64);
        gas as u128 * gas_price
    }

    fn gain(
        gas_price: Balance,
        depth: usize,
        input_value: MilliNear,
        output_value: Balance,
    ) -> u128 {
        let input_value = input_value.to_yocto();
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
pub struct PreviewList {
    pub input_value: MilliNear,
    pub list: Vec<Preview>,
    pub total_gain: u128,
}

impl PreviewList {
    pub fn new(input_value: MilliNear, previews: Vec<Preview>) -> Option<Self> {
        let total_gain: u128 = previews.iter().map(|p| p.gain).sum();
        Some(PreviewList {
            input_value,
            list: previews,
            total_gain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::{TokenAccount, TokenOutAccount};

    fn token_out(token: &str) -> TokenOutAccount {
        let token: TokenAccount = token.parse().unwrap();
        token.into()
    }

    const HEAD: u128 = 270_000_000_000_000_000_000;
    const BY_STEP: u128 = 260_000_000_000_000_000_000;
    const MIN_GAS_PRICE: Balance = 100_000_000;

    #[test]
    fn test_preview_cost() {
        assert_eq!(Preview::cost(MIN_GAS_PRICE, 1), HEAD + BY_STEP);
        assert_eq!(Preview::cost(MIN_GAS_PRICE, 2), HEAD + 2 * BY_STEP);
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
                MilliNear::of(100),
                MilliNear::of(200).to_yocto()
            ),
            MilliNear::of(100).to_yocto() - HEAD - 2 * BY_STEP
        );
    }

    #[test]
    fn test_preview_list_total_gain() {
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
}
