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
    let deserialized: HashMap<TokenOutAccount, i32> = serde_json::from_str(&string_json).unwrap();
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
fn test_token_account_to_in() {
    let token: TokenAccount = "wrap.near".parse().unwrap();
    let token_in = token.to_in();
    assert_eq!(token_in.inner(), &token);
    assert_eq!(token_in.inner().as_str(), "wrap.near");
}

#[test]
fn test_token_account_to_out() {
    let token: TokenAccount = "wrap.near".parse().unwrap();
    let token_out = token.to_out();
    assert_eq!(token_out.inner(), &token);
    assert_eq!(token_out.inner().as_str(), "wrap.near");
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
