use std::str::FromStr;

use near_account_id::{AccountId, ParseAccountError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TokenAccount(AccountId);

impl TokenAccount {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_account_id(&self) -> &AccountId {
        &self.0
    }

    pub fn to_in(&self) -> TokenInAccount {
        TokenInAccount(self.clone())
    }

    pub fn to_out(&self) -> TokenOutAccount {
        TokenOutAccount(self.clone())
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
        Ok(TokenAccount(s.parse()?))
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

/// Quote token として使用するトークン（from/in 側）
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TokenInAccount(pub TokenAccount);

impl std::fmt::Display for TokenInAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenInAccount {
    type Err = ParseAccountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TokenInAccount(s.parse()?))
    }
}

impl From<TokenAccount> for TokenInAccount {
    fn from(value: TokenAccount) -> Self {
        TokenInAccount(value)
    }
}

impl From<AccountId> for TokenInAccount {
    fn from(value: AccountId) -> Self {
        TokenInAccount(TokenAccount(value))
    }
}

impl From<TokenInAccount> for TokenAccount {
    fn from(value: TokenInAccount) -> Self {
        value.0
    }
}

impl From<TokenInAccount> for AccountId {
    fn from(value: TokenInAccount) -> Self {
        value.0.into()
    }
}

impl TokenInAccount {
    /// 内部の TokenAccount への参照を取得
    pub fn inner(&self) -> &TokenAccount {
        &self.0
    }

    pub fn as_account_id(&self) -> &AccountId {
        self.0.as_account_id()
    }

    pub fn as_out(&self) -> TokenOutAccount {
        TokenOutAccount(self.0.clone())
    }
}

/// Base token として使用するトークン（to/out 側）
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(transparent)]
pub struct TokenOutAccount(pub TokenAccount);

impl std::fmt::Display for TokenOutAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenOutAccount {
    type Err = ParseAccountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TokenOutAccount(s.parse()?))
    }
}

impl From<TokenAccount> for TokenOutAccount {
    fn from(value: TokenAccount) -> Self {
        TokenOutAccount(value)
    }
}

impl From<AccountId> for TokenOutAccount {
    fn from(value: AccountId) -> Self {
        TokenOutAccount(TokenAccount(value))
    }
}

impl From<TokenOutAccount> for TokenAccount {
    fn from(value: TokenOutAccount) -> Self {
        value.0
    }
}

impl From<TokenOutAccount> for AccountId {
    fn from(value: TokenOutAccount) -> Self {
        value.0.into()
    }
}

impl TokenOutAccount {
    /// 内部の TokenAccount への参照を取得
    pub fn inner(&self) -> &TokenAccount {
        &self.0
    }

    pub fn as_account_id(&self) -> &AccountId {
        self.0.as_account_id()
    }

    pub fn as_in(&self) -> TokenInAccount {
        TokenInAccount(self.0.clone())
    }
}

#[cfg(test)]
mod tests;
