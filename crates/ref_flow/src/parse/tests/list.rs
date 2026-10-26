//! List page items (`/v3/accounts/{ref}/receipts`).

use super::*;

#[test]
fn keeps_swap_and_ft_receipts_of_the_ref_contract_only() {
    let items = list_fixture();
    assert_eq!(items.len(), 6);
    let parsed: Vec<Option<RefReceipt>> =
        items.iter().map(|v| parse_list_item(v).unwrap()).collect();

    let swap_ok = parsed[0].as_ref().unwrap();
    assert_eq!(swap_ok.methods(), ["swap"]);
    assert!(swap_ok.ok);
    assert_eq!(
        swap_ok.receipt_id.as_str(),
        "5CdCpY8w5oEmTamMyWtx9PxwtGenM7nm9hNbw8A4kLq1"
    );
    assert!(swap_ok.block_height > 221_000_000);
    assert_eq!(
        swap_ok.block_time.and_utc().timestamp() / 86_400,
        1_792_843_229 / 86_400
    );

    let swap_failed = parsed[1].as_ref().unwrap();
    assert!(!swap_failed.ok);
    assert_eq!(swap_failed.methods(), ["swap"]);

    let ft_ok = parsed[2].as_ref().unwrap();
    assert_eq!(ft_ok.methods(), ["ft_on_transfer"]);
    assert_ne!(ft_ok.predecessor.as_str(), REF_CONTRACT);

    let swap_withdraw = parsed[3].as_ref().unwrap();
    assert_eq!(swap_withdraw.methods(), ["swap", "withdraw"]);

    assert!(parsed[4].is_none(), "receiver is not the Ref contract");
    assert!(parsed[5].is_none(), "add_liquidity is not collected");
}

#[test]
fn invalid_fields_are_errors() {
    let mut item = list_fixture()[0].clone();
    item["receipt_id"] = Value::String("0OIl".to_owned());
    assert!(matches!(
        parse_list_item(&item).unwrap_err(),
        ListParseError::Invalid {
            field: "receipt_id",
            ..
        }
    ));

    let mut item = list_fixture()[0].clone();
    item["predecessor_account_id"] = Value::String("Bad Account".to_owned());
    assert!(matches!(
        parse_list_item(&item).unwrap_err(),
        ListParseError::Invalid {
            field: "predecessor_account_id",
            ..
        }
    ));

    let mut item = list_fixture()[0].clone();
    item["block"] = Value::Null;
    assert!(matches!(
        parse_list_item(&item).unwrap_err(),
        ListParseError::Shape(_)
    ));

    assert!(matches!(
        parse_list_item(&Value::String("x".to_owned())).unwrap_err(),
        ListParseError::Shape(_)
    ));
}

#[test]
fn actions_without_a_method_are_dropped_from_methods() {
    let mut item = list_fixture()[0].clone();
    item["actions"] = serde_json::json!([{"action": "TRANSFER", "method": null}, {"action": "FUNCTION_CALL", "method": "swap"}]);
    let receipt = parse_list_item(&item).unwrap().unwrap();
    assert_eq!(receipt.methods(), ["swap"]);
}
