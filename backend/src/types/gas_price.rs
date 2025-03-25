use near_primitives::types::Balance;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct GasPrice(u64);

impl GasPrice {
    pub const fn from_balance(balance: Balance) -> Self {
        GasPrice(balance as u64)
    }

    pub const fn to_balance(self) -> Balance {
        self.0 as Balance
    }
}
