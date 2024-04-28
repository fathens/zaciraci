use std::fmt::Display;

#[derive(Debug, PartialEq)]
pub enum Error {
    TokenIdsNotTwo(usize),
    AmountsNotTwo(usize),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::TokenIdsNotTwo(n) => write!(f, "Expected 2 token_account_ids, got {}", n),
            Error::AmountsNotTwo(n) => write!(f, "Expected 2 amounts, got {}", n),
        }
    }
}
