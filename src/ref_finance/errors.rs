use near_jsonrpc_primitives::types::query::QueryResponseKind;
use std::fmt::{Debug, Display};

#[derive(Debug)]
pub enum Error {
    SwapSameToken,
    ZeroAmount,
    Overflow,
    OutOfIndexOfPools(usize),
    OutOfIndexOfTokens(usize),
    DifferentLengthOfTokens(usize, usize),
    InvalidPoolSize(usize),
    UnknownResponse(QueryResponseKind),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::SwapSameToken => write!(f, "Cannot swap the same token"),
            Error::ZeroAmount => write!(f, "Cannot handle zero amount"),
            Error::OutOfIndexOfTokens(index) => write!(f, "Out of index of tokens: {}", index),
            Error::OutOfIndexOfPools(index) => write!(f, "Out of index of pools: {}", index),
            Error::Overflow => write!(f, "Overflow"),
            Error::InvalidPoolSize(n) => write!(f, "Invalid pool size: {}", n),
            Error::DifferentLengthOfTokens(token_ids, amounts) => write!(
                f,
                "Different length of tokens: {} and {}",
                token_ids, amounts
            ),
            Error::UnknownResponse(kind) => write!(f, "Unknown response: {:?}", kind),
        }
    }
}
