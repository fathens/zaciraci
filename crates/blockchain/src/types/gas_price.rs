use near_sdk::NearToken;

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct GasPrice(u64);

impl GasPrice {
    pub const fn from_balance(balance: NearToken) -> Self {
        GasPrice(balance.as_yoctonear() as u64)
    }

    /// Returns the gas price as yoctoNEAR for arithmetic operations.
    pub const fn to_balance(self) -> u128 {
        self.0 as u128
    }
}
