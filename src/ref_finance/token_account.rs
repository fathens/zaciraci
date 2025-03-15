use crate::jsonrpc::IS_MAINNET;
use near_primitives::account::id::ParseAccountError;
use near_sdk::AccountId;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub static WNEAR_TOKEN: Lazy<TokenAccount> = Lazy::new(|| {
    let id = if *IS_MAINNET {
        "wrap.near"
    } else {
        "wrap.testnet"
    };
    TokenAccount(id.parse().unwrap())
});

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenAccount(AccountId);

impl TokenAccount {
    pub fn as_id(&self) -> &AccountId {
        &self.0
    }
}

impl std::fmt::Display for TokenAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenAccount {
    type Err = ParseAccountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse().map(TokenAccount)
    }
}

impl From<AccountId> for TokenAccount {
    fn from(value: AccountId) -> Self {
        TokenAccount(value)
    }
}

impl From<TokenAccount> for AccountId {
    fn from(value: TokenAccount) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenInAccount(TokenAccount);

impl TokenInAccount {
    pub fn as_id(&self) -> &AccountId {
        &self.0 .0
    }

    pub fn as_account(&self) -> &TokenAccount {
        &self.0
    }

    pub fn as_out(&self) -> TokenOutAccount {
        TokenOutAccount(self.0.clone())
    }
}

impl std::fmt::Display for TokenInAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<AccountId> for TokenInAccount {
    fn from(value: AccountId) -> Self {
        TokenInAccount(TokenAccount(value))
    }
}

impl From<TokenAccount> for TokenInAccount {
    fn from(value: TokenAccount) -> Self {
        TokenInAccount(value)
    }
}

impl From<TokenInAccount> for TokenAccount {
    fn from(value: TokenInAccount) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenOutAccount(TokenAccount);

impl TokenOutAccount {
    pub fn as_id(&self) -> &AccountId {
        &self.0 .0
    }

    pub fn as_account(&self) -> &TokenAccount {
        &self.0
    }

    pub fn as_in(&self) -> TokenInAccount {
        TokenInAccount(self.0.clone())
    }
}

impl std::fmt::Display for TokenOutAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<AccountId> for TokenOutAccount {
    fn from(value: AccountId) -> Self {
        TokenOutAccount(TokenAccount(value))
    }
}

impl From<TokenAccount> for TokenOutAccount {
    fn from(value: TokenAccount) -> Self {
        TokenOutAccount(value)
    }
}

impl From<TokenOutAccount> for TokenAccount {
    fn from(value: TokenOutAccount) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_account() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        let account = token.0.clone();
        assert_eq!(token.as_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_in_account() {
        let base: TokenAccount = "wrap.near".parse().unwrap();
        let account = base.0.clone();
        let token: TokenInAccount = base.into();
        assert_eq!(token.as_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_out_account() {
        let base = TokenAccount::from_str("wrap.near").unwrap();
        let account = base.0.clone();
        let token: TokenOutAccount = base.into();
        assert_eq!(token.as_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }
}
