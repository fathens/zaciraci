use super::*;
use std::collections::HashSet;

/// Real mainnet values captured from NearBlocks (public on-chain ids).
const TX: &str = "6YzxCS3V2yFFaVc1oSG5C9Y5VL6Md5kytehV9MHpERKt";
const RECEIPT: &str = "AtQMD91kr9UgmWY5ghBuV68u6APu2jTC8qawZVN9wQhx";

fn encode(bytes: &[u8]) -> String {
    bs58::encode(bytes).into_string()
}

#[test]
fn accepts_real_tx_hash_and_receipt_id() {
    let tx = TxHash::new(TX).unwrap();
    let rid = ReceiptId::new(RECEIPT).unwrap();
    assert_eq!(tx.as_str(), TX);
    assert_eq!(rid.as_str(), RECEIPT);
    assert_eq!(tx.to_string(), TX);
    assert_eq!(String::from(rid), RECEIPT);
}

#[test]
fn accepts_shortest_and_longest_encodings_of_32_bytes() {
    // All-zero bytes encode to 32 leading '1' characters.
    let zeros = encode(&[0u8; 32]);
    assert_eq!(zeros.len(), 32);
    assert!(TxHash::new(&zeros).is_ok());
    // All-0xff bytes encode to the maximum 44 characters.
    let max = encode(&[0xffu8; 32]);
    assert_eq!(max.len(), 44);
    assert!(TxHash::new(&max).is_ok());
}

#[test]
fn rejects_wrong_decoded_length() {
    assert_eq!(
        TxHash::new(&encode(&[7u8; 31])).unwrap_err(),
        ParseHashError::InvalidLength(31)
    );
    assert_eq!(
        ReceiptId::new(&encode(&[7u8; 33])).unwrap_err(),
        ParseHashError::InvalidLength(33)
    );
}

#[test]
fn rejects_empty_and_too_long() {
    assert_eq!(TxHash::new("").unwrap_err(), ParseHashError::Empty);
    let long = "1".repeat(MAX_ENCODED_LEN + 1);
    assert_eq!(
        TxHash::new(&long).unwrap_err(),
        ParseHashError::TooLong(MAX_ENCODED_LEN + 1)
    );
}

#[test]
fn rejects_characters_outside_base58_alphabet() {
    // '0', 'O', 'I', 'l' are excluded from the Bitcoin base58 alphabet.
    for bad in ['0', 'O', 'I', 'l'] {
        let mut s = TX.to_owned();
        s.replace_range(0..1, &bad.to_string());
        assert_eq!(
            TxHash::new(&s).unwrap_err(),
            ParseHashError::InvalidCharacter,
            "{bad:?} must be rejected"
        );
    }
}

#[test]
fn rejects_url_delimiters_and_path_traversal() {
    for s in [
        "../etc/passwd",
        "a?b=1",
        "x#frag",
        "abc/def",
        "6YzxCS3V2yFFaVc1oSG5C9Y5VL6Md5kytehV9MHpERKt/receipts",
    ] {
        assert_eq!(
            TxHash::new(s).unwrap_err(),
            ParseHashError::InvalidCharacter,
            "{s:?} must be rejected"
        );
    }
}

#[test]
fn rejects_surrounding_whitespace_without_trimming() {
    assert_eq!(
        TxHash::new(&format!(" {TX}")).unwrap_err(),
        ParseHashError::InvalidCharacter
    );
    assert_eq!(
        TxHash::new(&format!("{TX}\n")).unwrap_err(),
        ParseHashError::InvalidCharacter
    );
}

#[test]
fn from_str_matches_new() {
    let a: TxHash = TX.parse().unwrap();
    let b = TxHash::new(TX).unwrap();
    assert_eq!(a, b);
    assert_eq!(
        "not base58!".parse::<ReceiptId>().unwrap_err(),
        ParseHashError::InvalidCharacter
    );
}

#[test]
fn usable_as_set_key() {
    let mut set = HashSet::new();
    assert!(set.insert(TxHash::new(TX).unwrap()));
    assert!(!set.insert(TxHash::new(TX).unwrap()));
    assert!(set.insert(TxHash::new(&encode(&[1u8; 32])).unwrap()));
    assert_eq!(set.len(), 2);
}

#[test]
fn serde_round_trip_and_validation() {
    let tx = TxHash::new(TX).unwrap();
    let json = serde_json::to_string(&tx).unwrap();
    assert_eq!(json, format!("\"{TX}\""));
    let back: TxHash = serde_json::from_str(&json).unwrap();
    assert_eq!(back, tx);
    let err = serde_json::from_str::<ReceiptId>("\"0OIl\"").unwrap_err();
    assert!(
        err.to_string().contains("base58"),
        "error should mention the alphabet: {err}"
    );
}

#[test]
fn error_messages_are_descriptive() {
    assert_eq!(
        ParseHashError::InvalidLength(31).to_string(),
        "hash decodes to 31 bytes, expected 32"
    );
    assert_eq!(
        ParseHashError::TooLong(65).to_string(),
        "hash is 65 characters long, longer than the 64 allowed"
    );
}
