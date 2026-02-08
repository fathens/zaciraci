use crate::token_index::TokenIndex;
use common::types::{TokenInAccount, TokenOutAccount};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Cannot find token account: {0}")]
    TokenNotFound(String),
    #[error("Cannot swap the same token")]
    SwapSameToken,
    #[error("Cannot handle zero amount")]
    ZeroAmount,
    #[error("Overflow")]
    Overflow,
    #[error("Out of index of pools: {0}")]
    OutOfIndexOfPools(u32),
    #[error("Out of index of tokens: {0}")]
    OutOfIndexOfTokens(TokenIndex),
    #[error("Different length of tokens: {0} and {1}")]
    DifferentLengthOfTokens(usize, usize),
    #[error("Unmatched token path: {0:?} and {1:?}")]
    UnmatchedTokenPath(
        (TokenInAccount, TokenOutAccount),
        (TokenInAccount, TokenOutAccount),
    ),
}
