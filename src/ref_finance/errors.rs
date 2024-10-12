use crate::ref_finance::token_account::{TokenAccount, TokenInAccount, TokenOutAccount};
use crate::ref_finance::token_index::TokenIndex;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use std::fmt::{Debug, Display};

#[derive(Debug)]
pub enum Error {
    SwapSameToken,
    ZeroAmount,
    Overflow,
    OutOfIndexOfPools(u32),
    OutOfIndexOfTokens(TokenIndex),
    DifferentLengthOfTokens(usize, usize),
    InvalidPoolSize(usize),
    InvalidTokenAccountId(String),
    TokenNotFound(TokenAccount),
    NoValidPathFromToken(TokenInAccount),
    NoValidEddge(TokenInAccount, TokenOutAccount),
    UnknownResponse(QueryResponseKind),
    UnmatchedTokenPath(
        (TokenInAccount, TokenOutAccount),
        (TokenInAccount, TokenOutAccount),
    ),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::SwapSameToken => write!(f, "Cannot swap the same token"),
            Error::ZeroAmount => write!(f, "Cannot handle zero amount"),
            Error::TokenNotFound(token) => write!(f, "Token not found: {}", token),
            Error::OutOfIndexOfTokens(index) => write!(f, "Out of index of tokens: {}", index),
            Error::OutOfIndexOfPools(index) => write!(f, "Out of index of pools: {}", index),
            Error::Overflow => write!(f, "Overflow"),
            Error::InvalidPoolSize(n) => write!(f, "Invalid pool size: {}", n),
            Error::InvalidTokenAccountId(msg) => write!(f, "Invalid token account ID: {}", msg),
            Error::NoValidPathFromToken(token) => write!(f, "No valid path from token: {}", token),
            Error::NoValidEddge(token_in, token_out) => write!(
                f,
                "No valid edge from token in: {} to token out: {}",
                token_in, token_out
            ),
            Error::DifferentLengthOfTokens(token_ids, amounts) => write!(
                f,
                "Different length of tokens: {} and {}",
                token_ids, amounts
            ),
            Error::UnknownResponse(kind) => write!(f, "Unknown response: {:?}", kind),
            Error::UnmatchedTokenPath((token_in, token_out), (token_in2, token_out2)) => write!(
                f,
                "Unmatched token path: ({} -> {}) and ({} -> {})",
                token_in, token_out, token_in2, token_out2
            ),
        }
    }
}
