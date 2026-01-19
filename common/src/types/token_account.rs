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

/// Quote token として使用するトークン（from/in 側）
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenInAccount(pub TokenAccount);

impl std::fmt::Display for TokenInAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenInAccount {
    type Err = std::str::Utf8Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TokenInAccount(s.parse()?))
    }
}

impl From<TokenAccount> for TokenInAccount {
    fn from(value: TokenAccount) -> Self {
        TokenInAccount(value)
    }
}

impl TokenInAccount {
    /// 内部の TokenAccount への参照を取得
    pub fn inner(&self) -> &TokenAccount {
        &self.0
    }
}

/// Base token として使用するトークン（to/out 側）
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
pub struct TokenOutAccount(pub TokenAccount);

impl std::fmt::Display for TokenOutAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TokenOutAccount {
    type Err = std::str::Utf8Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(TokenOutAccount(s.parse()?))
    }
}

impl From<TokenAccount> for TokenOutAccount {
    fn from(value: TokenAccount) -> Self {
        TokenOutAccount(value)
    }
}

impl TokenOutAccount {
    /// 内部の TokenAccount への参照を取得
    pub fn inner(&self) -> &TokenAccount {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

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

    // ==================== Phase 0: シリアライズテスト ====================

    #[test]
    fn test_token_out_account_serialization() {
        // 単体シリアライズが String と同じ形式
        let token: TokenOutAccount = "wrap.near".parse().unwrap();
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"wrap.near\"");

        let deserialized: TokenOutAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(token, deserialized);
    }

    #[test]
    fn test_token_in_account_serialization() {
        let token: TokenInAccount = "wrap.near".parse().unwrap();
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"wrap.near\"");

        let deserialized: TokenInAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(token, deserialized);
    }

    #[test]
    fn test_token_out_account_as_hashmap_key() {
        // String キーの HashMap
        let mut string_map: HashMap<String, i32> = HashMap::new();
        string_map.insert("wrap.near".to_string(), 100);
        let string_json = serde_json::to_string(&string_map).unwrap();

        // TokenOutAccount キーの HashMap
        let mut token_map: HashMap<TokenOutAccount, i32> = HashMap::new();
        token_map.insert("wrap.near".parse().unwrap(), 100);
        let token_json = serde_json::to_string(&token_map).unwrap();

        // 同じ JSON 形式であることを確認
        assert_eq!(string_json, token_json);

        // String キーの JSON から TokenOutAccount キーに deserialize できることを確認
        let deserialized: HashMap<TokenOutAccount, i32> =
            serde_json::from_str(&string_json).unwrap();
        assert_eq!(
            deserialized.get(&"wrap.near".parse::<TokenOutAccount>().unwrap()),
            Some(&100)
        );
    }

    #[test]
    fn test_token_out_account_as_btreemap_key() {
        let mut string_map: BTreeMap<String, f64> = BTreeMap::new();
        string_map.insert("token1".to_string(), 0.5);
        string_map.insert("token2".to_string(), 0.5);
        let string_json = serde_json::to_string(&string_map).unwrap();

        let mut token_map: BTreeMap<TokenOutAccount, f64> = BTreeMap::new();
        token_map.insert("token1".parse().unwrap(), 0.5);
        token_map.insert("token2".parse().unwrap(), 0.5);
        let token_json = serde_json::to_string(&token_map).unwrap();

        assert_eq!(string_json, token_json);
    }
}
