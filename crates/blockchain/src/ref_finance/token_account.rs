use crate::jsonrpc::IS_MAINNET;
use common::types::TokenAccount;
use once_cell::sync::Lazy;

pub static WNEAR_TOKEN: Lazy<TokenAccount> = Lazy::new(|| {
    let id = if *IS_MAINNET {
        "wrap.near"
    } else {
        "wrap.testnet"
    };
    id.parse().unwrap()
});

/// ネイティブ NEAR を表す特別なトークン（mainnet/testnet で同じ）
pub static NEAR_TOKEN: Lazy<TokenAccount> = Lazy::new(|| "near".parse().unwrap());

#[cfg(test)]
mod tests {
    use super::*;
    use common::types::{AccountId, TokenInAccount, TokenOutAccount};
    use std::str::FromStr;

    #[test]
    fn test_token_account() {
        let token: TokenAccount = "wrap.near".parse().unwrap();
        let account = token.as_account_id().clone();
        assert_eq!(token.as_account_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_in_account() {
        let base: TokenAccount = "wrap.near".parse().unwrap();
        let account = base.as_account_id().clone();
        let token: TokenInAccount = base.into();
        assert_eq!(token.as_account_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }

    #[test]
    fn test_token_out_account() {
        let base = TokenAccount::from_str("wrap.near").unwrap();
        let account = base.as_account_id().clone();
        let token: TokenOutAccount = base.into();
        assert_eq!(token.as_account_id(), &account);
        assert_eq!(token.to_string(), "wrap.near");
    }

    #[test]
    fn test_account_id_conversion() {
        // AccountId から TokenAccount への変換
        let account_id: AccountId = "wrap.near".parse().unwrap();
        let token = TokenAccount::from(account_id.clone());
        assert_eq!(token.as_account_id(), &account_id);

        // TokenAccount から AccountId への変換
        let token2: TokenAccount = "test.near".parse().unwrap();
        let account_id2: AccountId = token2.into();
        assert_eq!(account_id2.as_str(), "test.near");
    }

    #[test]
    fn test_token_in_out_conversion() {
        // TokenInAccount から TokenOutAccount への変換
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        let token_out = token_in.as_out();
        assert_eq!(token_out.to_string(), "wrap.near");

        // TokenOutAccount から TokenInAccount への変換
        let token_out2: TokenOutAccount = "test.near".parse().unwrap();
        let token_in2 = token_out2.as_in();
        assert_eq!(token_in2.to_string(), "test.near");
    }

    #[test]
    fn test_inner_method() {
        // inner() メソッドのテスト
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        assert_eq!(token_in.inner().as_str(), "wrap.near");

        let token_out: TokenOutAccount = "wrap.near".parse().unwrap();
        assert_eq!(token_out.inner().as_str(), "wrap.near");
    }

    #[test]
    fn test_token_in_out_account_from_account_id() {
        // AccountId から TokenInAccount/TokenOutAccount への変換
        let account_id: AccountId = "wrap.near".parse().unwrap();
        let token_in = TokenInAccount::from(account_id.clone());
        let token_out = TokenOutAccount::from(account_id.clone());

        assert_eq!(token_in.as_account_id(), &account_id);
        assert_eq!(token_out.as_account_id(), &account_id);
    }

    #[test]
    fn test_token_in_out_account_into_account_id() {
        // TokenInAccount/TokenOutAccount から AccountId への変換
        let token_in: TokenInAccount = "wrap.near".parse().unwrap();
        let account_id1: AccountId = token_in.into();
        assert_eq!(account_id1.as_str(), "wrap.near");

        let token_out: TokenOutAccount = "test.near".parse().unwrap();
        let account_id2: AccountId = token_out.into();
        assert_eq!(account_id2.as_str(), "test.near");
    }
}
