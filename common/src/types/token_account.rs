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
mod tests {
    use super::*;
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn test_token_account_from_str() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        assert_eq!(token.as_str(), "wrap.near");
        assert_eq!(token.to_string(), "wrap.near".to_owned());
    }

    #[test]
    fn test_token_account_from_json() {
        let token: TokenAccount = serde_json::from_str("\"wrap.near\"").unwrap();
        assert_eq!(token.as_str(), "wrap.near");
        assert_eq!(token.to_string(), "wrap.near".to_owned());
    }

    #[test]
    fn test_token_account_as_account_id() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        let account_id: &AccountId = token.as_account_id();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_account_from_account_id() {
        let account_id: AccountId = "wrap.near".parse().unwrap();
        let token = TokenAccount::from(account_id.clone());
        assert_eq!(token.as_account_id(), &account_id);
    }

    #[test]
    fn test_token_account_into_account_id() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        let account_id: AccountId = token.into();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_invalid_token_account() {
        // 無効なアカウント名はエラーになる
        let result: Result<TokenAccount, _> = "INVALID_UPPER_CASE".parse();
        assert!(result.is_err());
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
        string_map.insert("token1.near".to_string(), 0.5);
        string_map.insert("token2.near".to_string(), 0.5);
        let string_json = serde_json::to_string(&string_map).unwrap();

        let mut token_map: BTreeMap<TokenOutAccount, f64> = BTreeMap::new();
        token_map.insert("token1.near".parse().unwrap(), 0.5);
        token_map.insert("token2.near".parse().unwrap(), 0.5);
        let token_json = serde_json::to_string(&token_map).unwrap();

        assert_eq!(string_json, token_json);
    }

    // ==================== Phase 3: TokenInAccount / TokenOutAccount メソッドテスト ====================

    #[test]
    fn test_token_in_account_as_account_id() {
        let token: TokenInAccount = "wrap.near".parse().unwrap();
        let account_id = token.as_account_id();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_in_account_as_out() {
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        let token_out = token_in.as_out();
        assert_eq!(token_out.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_out_account_as_account_id() {
        let token: TokenOutAccount = "wrap.near".parse().unwrap();
        let account_id = token.as_account_id();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_out_account_as_in() {
        let token_out: TokenOutAccount = "wrap.near".parse().unwrap();
        let token_in = token_out.as_in();
        assert_eq!(token_in.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_in_out_conversion() {
        let original: TokenInAccount = "token.near".parse().unwrap();
        let converted = original.as_out().as_in();
        assert_eq!(original, converted);
    }

    // ==================== Phase 3: 追加テスト ====================

    #[test]
    fn test_token_account_json_serialize() {
        // TokenAccount の JSON シリアライズ
        let token: TokenAccount = "wrap.near".parse().unwrap();
        let json = serde_json::to_string(&token).unwrap();
        assert_eq!(json, "\"wrap.near\"");
    }

    #[test]
    fn test_token_in_account_inner() {
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        let inner = token_in.inner();
        assert_eq!(inner.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_out_account_inner() {
        let token_out: TokenOutAccount = "wrap.near".parse().unwrap();
        let inner = token_out.inner();
        assert_eq!(inner.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_in_account_from_account_id() {
        let account_id: AccountId = "wrap.near".parse().unwrap();
        let token_in = TokenInAccount::from(account_id.clone());
        assert_eq!(token_in.as_account_id(), &account_id);
    }

    #[test]
    fn test_token_out_account_from_account_id() {
        let account_id: AccountId = "wrap.near".parse().unwrap();
        let token_out = TokenOutAccount::from(account_id.clone());
        assert_eq!(token_out.as_account_id(), &account_id);
    }

    #[test]
    fn test_token_in_account_into_account_id() {
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        let account_id: AccountId = token_in.into();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_token_out_account_into_account_id() {
        let token_out: TokenOutAccount = "wrap.near".parse().unwrap();
        let account_id: AccountId = token_out.into();
        assert_eq!(account_id.as_str(), "wrap.near");
    }

    #[test]
    fn test_invalid_token_in_account() {
        // 無効なアカウント名はエラーになる
        let result: Result<TokenInAccount, _> = "INVALID_UPPER_CASE".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_token_out_account() {
        // 無効なアカウント名はエラーになる
        let result: Result<TokenOutAccount, _> = "INVALID_UPPER_CASE".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_near_account_formats() {
        // 有効な NEAR アカウント形式のテスト
        let valid_accounts = [
            "wrap.near",
            "usdc.near",
            "aa", // 2文字の最短アカウント
            "near",
            "wrap.testnet",
            "user123.near",
            "sub.account.near",
            "user_name.near", // アンダースコア
            "user-name.near", // ハイフン
        ];

        for account in valid_accounts {
            let result: Result<TokenAccount, _> = account.parse();
            assert!(result.is_ok(), "Expected '{}' to be valid", account);
        }
    }

    #[test]
    fn test_invalid_near_account_formats() {
        // 無効な NEAR アカウント形式のテスト
        let invalid_accounts = [
            "",              // 空文字
            "A",             // 1文字（最低2文字必要）
            "UPPERCASE",     // 大文字
            "has space",     // スペース含む
            "has@symbol",    // 無効な記号
            &"a".repeat(65), // 長すぎる（最大64文字）
        ];

        for account in invalid_accounts {
            let result: Result<TokenAccount, _> = account.parse();
            assert!(result.is_err(), "Expected '{}' to be invalid", account);
        }
    }
}
