use crate::milli_near::MilliNear;
use crate::ref_finance::token_account::TokenOutAccount;
use near_gas::NearGas;

const MIN_GAIN: u128 = MilliNear::of(1).to_yocto();

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Preview {
    pub input_value: u128,
    pub token: TokenOutAccount,
    pub depth: usize,
    pub output_value: u128,
}

#[derive(Debug, Eq, PartialEq, Hash)]
pub struct PreviewList {
    input_value: u128,
    list: Vec<Preview>,
    total_gain: u128,
}

impl Preview {
    const HEAD: NearGas = NearGas::from_ggas(2700);
    const BY_STEP: NearGas = NearGas::from_ggas(2600);

    fn cost(&self) -> u128 {
        let gas_price = (MilliNear::of(1).to_yocto() / 10) / 10_u128.pow(12); // 0.0001 NEAR
        let gas = Self::HEAD.as_gas() + Self::BY_STEP.as_gas() * (self.depth as u64);
        gas as u128 * gas_price
    }

    pub fn gain(&self) -> u128 {
        if self.output_value <= self.input_value {
            return 0;
        }
        let gain = self.output_value - self.input_value;
        let cost = self.cost();
        if gain <= cost {
            return 0;
        }
        gain - cost
    }
}

impl PreviewList {
    pub fn new(input_value: u128, previews: Vec<Preview>) -> Option<Self> {
        let gains: u128 = previews.iter().map(|p| p.gain()).sum();
        if gains <= MIN_GAIN {
            return None;
        }
        let total_gain = gains - MIN_GAIN;
        Some(PreviewList {
            input_value,
            list: previews,
            total_gain,
        })
    }

    pub fn get_list(&self) -> Vec<Preview> {
        self.list.clone()
    }

    pub fn total_gain(&self) -> u128 {
        self.total_gain
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
    const MIN_GAIN: u128 = MilliNear::of(1).to_yocto();

    #[test]
    fn test_preview_cost() {
        assert_eq!(
            Preview {
                input_value: 0,
                token: token_out("a.token"),
                depth: 1,
                output_value: 0,
            }
            .cost(),
            HEAD + BY_STEP
        );

        assert_eq!(
            Preview {
                input_value: 0,
                token: token_out("a.token"),
                depth: 2,
                output_value: 0,
            }
            .cost(),
            HEAD + 2 * BY_STEP
        );
    }

    #[test]
    fn test_preview_gain() {
        assert_eq!(
            Preview {
                input_value: MilliNear::of(100).to_yocto(),
                token: token_out("a.token"),
                depth: 1,
                output_value: MilliNear::of(300).to_yocto(),
            }
            .gain(),
            MilliNear::of(200).to_yocto() - HEAD - BY_STEP
        );

        assert_eq!(
            Preview {
                input_value: MilliNear::of(100).to_yocto(),
                token: token_out("a.token"),
                depth: 2,
                output_value: MilliNear::of(200).to_yocto(),
            }
            .gain(),
            MilliNear::of(100).to_yocto() - HEAD - 2 * BY_STEP
        );
    }

    #[test]
    fn test_preview_list_total_gain() {
        let a = Preview {
            input_value: MilliNear::of(100).to_yocto(),
            token: token_out("a.token"),
            depth: 1,
            output_value: MilliNear::of(300).to_yocto(),
        };
        let b = Preview {
            input_value: MilliNear::of(100).to_yocto(),
            token: token_out("b.token"),
            depth: 1,
            output_value: MilliNear::of(200).to_yocto(),
        };
        let previews = vec![a.clone(), b.clone()];
        let preview_list = PreviewList::new(MilliNear::of(100).to_yocto(), previews).unwrap();
        assert_eq!(preview_list.total_gain(), a.gain() + b.gain() - MIN_GAIN);
    }
}
