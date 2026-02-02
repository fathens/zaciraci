//! NEAR API compatibility tests
//!
//! These tests verify that the NEAR library types work correctly together,
//! which is essential for ensuring the upgrade path for NEAR libraries.

use near_crypto::{InMemorySigner, KeyType};
use near_primitives::action::{Action, FunctionCallAction, TransferAction};
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::AccountId;
use near_sdk::json_types::U128;
use serde_json::json;

/// Test AccountId parsing from various string formats
#[test]
fn test_account_id_parsing() {
    // Standard account IDs
    let account1: AccountId = "test.near".parse().unwrap();
    assert_eq!(account1.as_str(), "test.near");

    let account2: AccountId = "alice.testnet".parse().unwrap();
    assert_eq!(account2.as_str(), "alice.testnet");

    // Sub-accounts
    let account3: AccountId = "sub.account.near".parse().unwrap();
    assert_eq!(account3.as_str(), "sub.account.near");

    // Implicit accounts (64 hex characters)
    let account4: AccountId = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        .parse()
        .unwrap();
    assert_eq!(
        account4.as_str(),
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    );

    // Invalid account IDs
    assert!("".parse::<AccountId>().is_err());
    assert!("UPPER.CASE".parse::<AccountId>().is_err()); // Must be lowercase
    assert!("-invalid.near".parse::<AccountId>().is_err()); // Cannot start with hyphen
}

/// Test U128 JSON serialization/deserialization
#[test]
fn test_u128_json_serialization() {
    // U128 serializes as a string in JSON
    let value = U128(1_000_000_000_000_000_000_000_000);
    let json_str = serde_json::to_string(&value).unwrap();
    assert_eq!(json_str, "\"1000000000000000000000000\"");

    // Deserialization from string
    let parsed: U128 = serde_json::from_str("\"1000000000000000000000000\"").unwrap();
    assert_eq!(parsed.0, 1_000_000_000_000_000_000_000_000);

    // Edge cases
    let zero = U128(0);
    let json_zero = serde_json::to_string(&zero).unwrap();
    assert_eq!(json_zero, "\"0\"");

    let max = U128(u128::MAX);
    let json_max = serde_json::to_string(&max).unwrap();
    let parsed_max: U128 = serde_json::from_str(&json_max).unwrap();
    assert_eq!(parsed_max.0, u128::MAX);

    // U128 in nested JSON
    let json_obj = json!({
        "amount": U128(1_000_000),
        "account_id": "test.near",
    });
    let amount = json_obj.get("amount").unwrap();
    assert_eq!(amount.as_str().unwrap(), "1000000");
}

/// Test Transaction creation and signing
#[test]
fn test_transaction_creation() {
    let signer_id: AccountId = "sender.near".parse().unwrap();
    let receiver_id: AccountId = "receiver.near".parse().unwrap();

    // Create a signer
    let signer_result = InMemorySigner::from_seed(signer_id.clone(), KeyType::ED25519, "test_seed");
    let signer = match signer_result {
        near_crypto::Signer::InMemory(s) => s,
        _ => panic!("Expected InMemorySigner"),
    };

    // Create transfer action
    let transfer_action = Action::Transfer(TransferAction {
        deposit: 1_000_000_000_000_000_000_000_000, // 1 NEAR
    });

    // Create function call action
    let function_call_action = Action::FunctionCall(
        FunctionCallAction {
            method_name: "test_method".to_string(),
            args: serde_json::to_vec(&json!({"key": "value"})).unwrap(),
            gas: 300_000_000_000_000, // 300 TGas
            deposit: 1,               // 1 yoctoNEAR for payable methods
        }
        .into(),
    );

    // Create transaction with multiple actions
    let nonce = 1;
    let block_hash = near_primitives::hash::CryptoHash::default();

    let transaction = Transaction::V0(TransactionV0 {
        signer_id: signer_id.clone(),
        public_key: signer.public_key(),
        nonce,
        receiver_id: receiver_id.clone(),
        block_hash,
        actions: vec![transfer_action, function_call_action],
    });

    // Get hash and sign
    let (hash, size) = transaction.get_hash_and_size();
    assert!(size > 0);

    let signature = signer.sign(hash.as_bytes());
    let signed_tx = SignedTransaction::new(signature.clone(), transaction.clone());

    // Verify signed transaction
    assert_eq!(signed_tx.transaction.signer_id(), &signer_id);
    assert_eq!(signed_tx.transaction.receiver_id(), &receiver_id);
    assert_eq!(signed_tx.transaction.actions().len(), 2);
}

/// Test InMemorySigner creation from different sources
#[test]
fn test_in_memory_signer_creation() {
    let account_id: AccountId = "test.near".parse().unwrap();

    // Create from seed
    let signer_from_seed =
        InMemorySigner::from_seed(account_id.clone(), KeyType::ED25519, "test_seed");
    let signer1 = match signer_from_seed {
        near_crypto::Signer::InMemory(s) => s,
        _ => panic!("Expected InMemorySigner"),
    };

    // Same seed should produce same key
    let signer_from_seed2 =
        InMemorySigner::from_seed(account_id.clone(), KeyType::ED25519, "test_seed");
    let signer2 = match signer_from_seed2 {
        near_crypto::Signer::InMemory(s) => s,
        _ => panic!("Expected InMemorySigner"),
    };

    assert_eq!(signer1.public_key(), signer2.public_key());

    // Different seed should produce different key
    let signer_from_different_seed =
        InMemorySigner::from_seed(account_id.clone(), KeyType::ED25519, "different_seed");
    let signer3 = match signer_from_different_seed {
        near_crypto::Signer::InMemory(s) => s,
        _ => panic!("Expected InMemorySigner"),
    };

    assert_ne!(signer1.public_key(), signer3.public_key());

    // Verify public key format
    let public_key = signer1.public_key();
    let key_string = public_key.to_string();
    assert!(key_string.starts_with("ed25519:"));

    // Verify signing works
    let message = b"test message";
    let signature = signer1.sign(message);
    assert!(signature.verify(message, &signer1.public_key()));
}
