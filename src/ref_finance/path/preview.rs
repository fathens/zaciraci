use crate::milli_near::MilliNear;
use crate::ref_finance::token_account::TokenOutAccount;

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
    const HEAD: u128 = 270_000_000_000_000_000_000;
    const BY_STEP: u128 = 260_000_000_000_000_000_000;

    fn cost(&self) -> u128 {
        Self::HEAD + Self::BY_STEP * (self.depth as u128)
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

const MIN_GAIN: u128 = MilliNear::of(1).to_yocto();

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

    #[test]
    fn test_preview_cost() {
        const HEAD: u128 = 270_000_000_000_000_000_000;
        const BY_STEP: u128 = 260_000_000_000_000_000_000;

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
            MilliNear::of(200).to_yocto() - Preview::HEAD - Preview::BY_STEP
        );

        assert_eq!(
            Preview {
                input_value: MilliNear::of(100).to_yocto(),
                token: token_out("a.token"),
                depth: 2,
                output_value: MilliNear::of(200).to_yocto(),
            }
            .gain(),
            MilliNear::of(100).to_yocto() - Preview::HEAD - 2 * Preview::BY_STEP
        );
    }

    #[test]
    fn test_preview_list_total_gain() {
        const MIN_GAIN: u128 = MilliNear::of(1).to_yocto();

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
