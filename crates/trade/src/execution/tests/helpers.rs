use bigdecimal::BigDecimal;
use common::types::{ExchangeRate, NearValue, TokenAccount};
use std::str::FromStr;

use super::{BuyOperation, SellOperation};

pub fn ta(s: &str) -> TokenAccount {
    s.parse().expect("invalid TokenAccount in test")
}

pub const RATE_24: &str = "500000000000000000000000";

pub fn sell_op(token: &str, near: i64, rate_raw: &str, decimals: u8) -> SellOperation {
    SellOperation {
        token: ta(token),
        near_value: NearValue::from_near(BigDecimal::from(near)),
        exchange_rate: ExchangeRate::from_raw_rate(
            BigDecimal::from_str(rate_raw).expect("invalid rate in test"),
            decimals,
        ),
    }
}

pub fn buy_op(token: &str, near: i64) -> BuyOperation {
    BuyOperation {
        token: ta(token),
        near_value: NearValue::from_near(BigDecimal::from(near)),
    }
}
