#[allow(dead_code)]

use bigdecimal::BigDecimal;

use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TokenRate {
    base : TokenInAccount,
    quote : TokenOutAccount,
    rate : BigDecimal,
}