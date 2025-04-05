use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenAccount(pub Box<str>);

impl std::fmt::Display for TokenAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenAccount {
    type Err = std::str::Utf8Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TokenAccount(s.to_string().into_boxed_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_account_from_str() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        assert_eq!(token.0.to_string(), "wrap.near".to_owned());
        assert_eq!(token.to_string(), "wrap.near".to_owned());
    }

    #[test]
    fn test_token_account_from_json() {
        let token: TokenAccount = serde_json::from_str("\"wrap.near\"").unwrap();
        assert_eq!(token.0.to_string(), "wrap.near".to_owned());
        assert_eq!(token.to_string(), "wrap.near".to_owned());
    }
}
