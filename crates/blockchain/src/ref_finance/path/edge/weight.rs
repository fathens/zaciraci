use dex::TokenPairId;
use num_traits::{one, zero};
use std::cmp::Ordering;
use std::ops::Add;

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct EdgeWeight {
    pair_id: Option<TokenPairId>,
    estimated_rate: f32,
}

impl EdgeWeight {
    fn calc_rate(input_value: u128, estimated_return: u128) -> f32 {
        if input_value == 0 {
            zero()
        } else {
            -(estimated_return as f32 / input_value as f32)
        }
    }

    pub fn new(pair_id: TokenPairId, input_value: u128, estimated_return: u128) -> Self {
        Self {
            pair_id: Some(pair_id),
            estimated_rate: Self::calc_rate(input_value, estimated_return),
        }
    }

    pub fn pair_id(&self) -> Option<TokenPairId> {
        self.pair_id
    }
}

impl Eq for EdgeWeight {}

impl Ord for EdgeWeight {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Equal)
    }
}

// NOTE: f64 を含むため、PartialOrd を先に実装し Ord がそれを使う設計
// NaN の場合は Equal として扱う
#[allow(clippy::non_canonical_partial_ord_impl)]
impl PartialOrd for EdgeWeight {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.estimated_rate.partial_cmp(&other.estimated_rate)
    }
}

impl Default for EdgeWeight {
    fn default() -> Self {
        EdgeWeight {
            pair_id: None,
            estimated_rate: one(),
        }
    }
}

// NOTE: グラフ経路計算用。エッジ重みの累積は乗算で行う（レートの合成）
// petgraph が Add trait を使用するため、Add で乗算を実装
#[allow(clippy::suspicious_arithmetic_impl)]
impl Add<EdgeWeight> for EdgeWeight {
    type Output = Self;
    fn add(self, rhs: EdgeWeight) -> Self::Output {
        EdgeWeight {
            pair_id: None,
            estimated_rate: self.estimated_rate * rhs.estimated_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weight(d: u128, n: u128) -> EdgeWeight {
        EdgeWeight {
            pair_id: None,
            estimated_rate: EdgeWeight::calc_rate(d, n),
        }
    }

    #[test]
    fn test_calc_rate() {
        assert_eq!(EdgeWeight::calc_rate(1, 1), -1.0);
        assert_eq!(EdgeWeight::calc_rate(1, 2), -2.0);
        assert_eq!(EdgeWeight::calc_rate(2, 1), -0.5);
        assert_eq!(EdgeWeight::calc_rate(2, 2), -1.0);
        assert_eq!(EdgeWeight::calc_rate(2, 0), 0.0,);
    }

    #[test]
    fn test_cmp() {
        let a = weight(1, 1);
        let b = weight(1, 2);
        let c = weight(2, 1);
        assert_eq!(a.cmp(&b), Ordering::Greater);
        assert_eq!(b.cmp(&a), Ordering::Less);
        assert_eq!(a.cmp(&a), Ordering::Equal);
        assert_eq!(a.cmp(&c), Ordering::Less);
        assert_eq!(c.cmp(&a), Ordering::Greater);
        assert_eq!(b.cmp(&c), Ordering::Less);
        assert_eq!(c.cmp(&b), Ordering::Greater);
    }

    #[test]
    fn test_add() {
        // 掛け算の結果を確認: (-1.0) * (-1.0) = 1.0
        let result1 = (weight(1, 1) + weight(1, 1)).estimated_rate;
        assert_eq!(result1, 1.0);

        // 掛け算の結果を確認: (-2.0) * (-0.5) = 1.0
        let result2 = (weight(1, 2) + weight(2, 1)).estimated_rate;
        assert_eq!(result2, 1.0);
    }
}
