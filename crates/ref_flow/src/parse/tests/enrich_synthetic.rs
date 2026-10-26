//! Hostile or unusual inputs built by editing the real fixtures.

use super::*;

fn parse_edited(name: &str, edit: impl FnOnce(&mut Value)) -> ParsedTx {
    let (tx_hash, mut roots) = enrich_fixture(name);
    edit(&mut roots[0]);
    parse_tx_receipts(&tx_hash, &roots).unwrap()
}

fn single_reject(parsed: &ParsedTx) -> &TradeRejectReason {
    assert!(
        parsed.trades.is_empty(),
        "no trade expected: {:?}",
        parsed.trades
    );
    assert_eq!(parsed.rejects.len(), 1, "{:?}", parsed.rejects);
    &parsed.rejects[0].reason
}

#[test]
fn ft_refund_rejects_the_receipt() {
    let parsed = parse_edited("ft_basic", |root| {
        ref_receipt_mut(root)["outcome"]["result"] = Value::String(b64_json("\"5\""));
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::FtRefund("5".to_owned())
    );
    assert_eq!(parsed.stats.rejects.get("ft_refund"), Some(&1));
}

#[test]
fn ft_refund_accepts_numeric_zero_result() {
    let parsed = parse_edited("ft_basic", |root| {
        ref_receipt_mut(root)["outcome"]["result"] = Value::String(b64_json("0"));
    });
    assert_eq!(parsed.trades.len(), 1);
}

#[test]
fn ft_without_result_is_rejected() {
    let parsed = parse_edited("ft_basic", |root| {
        ref_receipt_mut(root)["outcome"]["result"] = Value::Null;
    });
    assert_eq!(single_reject(&parsed), &TradeRejectReason::MissingRefund);
}

#[test]
fn log_count_must_match_the_actions() {
    let parsed = parse_edited("swap_chain", |root| {
        let logs = ref_receipt_mut(root)["outcome"]["logs"]
            .as_array_mut()
            .unwrap();
        logs.retain(|l| !l.as_str().unwrap().starts_with("Swapped 3027"));
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::LogCountMismatch {
            actions: 2,
            logs: 1
        }
    );
}

#[test]
fn log_tokens_must_match_the_action() {
    let parsed = parse_edited("swap_single", |root| {
        let logs = ref_receipt_mut(root)["outcome"]["logs"]
            .as_array_mut()
            .unwrap();
        logs[0] = Value::String("Swapped 2991757 zec.omft.near for 1 usdc.near".to_owned());
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::LogTokenMismatch { index: 0 }
    );
}

#[test]
fn malformed_logs_are_rejected() {
    for bad in [
        "Swapped 2991757 zec.omft.near for 7682134962619246593392108",
        "Swapped 2991757 zec.omft.near to 1 wrap.near",
        "Swapped 2991757 zec.omft.near for 1234567890123456789012345678901234567890 wrap.near",
        "Swapped -1 zec.omft.near for 1 wrap.near",
        "Swapped 1.5 zec.omft.near for 1 wrap.near",
        "Swapped 2991757 zec.omft.near for 1 Not A Token",
    ] {
        let parsed = parse_edited("swap_single", |root| {
            ref_receipt_mut(root)["outcome"]["logs"][0] = Value::String(bad.to_owned());
        });
        let reason = single_reject(&parsed);
        assert!(
            matches!(
                reason,
                TradeRejectReason::MalformedLog(_) | TradeRejectReason::InvalidToken(_)
            ),
            "{bad:?} -> {reason:?}"
        );
    }
}

#[test]
fn too_many_actions_or_logs_are_rejected() {
    let parsed = parse_edited("swap_single", |root| {
        let r = ref_receipt_mut(root);
        let action = r["actions"][0]["args"]["args_json"]["actions"][0].clone();
        let actions: Vec<Value> = std::iter::repeat_n(action, MAX_HOPS + 1).collect();
        r["actions"][0]["args"]["args_json"]["actions"] = Value::Array(actions);
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::TooManyActions(MAX_HOPS + 1)
    );

    let parsed = parse_edited("swap_single", |root| {
        let r = ref_receipt_mut(root);
        let logs: Vec<Value> =
            std::iter::repeat_n(Value::String("x".to_owned()), MAX_LOG_LINES + 1).collect();
        r["outcome"]["logs"] = Value::Array(logs);
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::TooManyLogs(MAX_LOG_LINES + 1)
    );
}

#[test]
fn oversized_msg_is_rejected_before_parsing() {
    let parsed = parse_edited("ft_basic", |root| {
        let r = ref_receipt_mut(root);
        r["actions"][0]["args"]["args_json"]["msg"] = Value::String("x".repeat(MAX_MSG_LEN + 1));
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::MsgTooLong(MAX_MSG_LEN + 1)
    );
}

#[test]
fn missing_or_empty_actions_are_rejected() {
    let parsed = parse_edited("swap_single", |root| {
        ref_receipt_mut(root)["actions"][0]["args"]["args_json"]["actions"] = Value::Array(vec![]);
    });
    assert_eq!(single_reject(&parsed), &TradeRejectReason::NoActions);

    let parsed = parse_edited("swap_single", |root| {
        ref_receipt_mut(root)["actions"][0]["args"]["args_json"] =
            Value::String("garbage".to_owned());
    });
    assert!(matches!(
        single_reject(&parsed),
        TradeRejectReason::MalformedArgs(_)
    ));
}

#[test]
fn invalid_ids_and_accounts_are_rejected_per_receipt() {
    let parsed = parse_edited("swap_single", |root| {
        ref_receipt_mut(root)["receipt_id"] = Value::String("0OIl".to_owned());
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::InvalidReceiptId("0OIl".to_owned())
    );

    let parsed = parse_edited("swap_single", |root| {
        ref_receipt_mut(root)["predecessor_account_id"] = Value::String("Not Valid".to_owned());
    });
    assert_eq!(
        single_reject(&parsed),
        &TradeRejectReason::InvalidSender("Not Valid".to_owned())
    );

    let parsed = parse_edited("swap_single", |root| {
        ref_receipt_mut(root)["block"] = Value::Null;
    });
    assert_eq!(single_reject(&parsed), &TradeRejectReason::MissingBlock);
}

#[test]
fn receipts_from_other_contracts_are_ignored_and_other_ref_methods_counted() {
    let parsed = parse_edited("swap_single", |root| {
        root["actions"][0]["method"] = Value::String("add_liquidity".to_owned());
    });
    assert!(parsed.trades.is_empty());
    assert!(parsed.rejects.is_empty());
    assert_eq!(parsed.stats.other_receipts, 1);
}

#[test]
fn structural_problems_are_errors_not_rejects() {
    let tx_hash = TxHash::new("6LAHAus8voUyKFffiFri6UX6yMPCtsctqRzLzmvyrmYM").unwrap();
    let err = parse_tx_receipts(&tx_hash, &[Value::String("nope".to_owned())]).unwrap_err();
    assert!(matches!(err, TxParseError::Shape(_)));

    // A wide tree of more than MAX_RECEIPTS_PER_TX nodes is refused.
    let (tx_hash, mut roots) = enrich_fixture("swap_single");
    let leaf = serde_json::json!({
        "receipt_id": "x", "predecessor_account_id": "a.near", "receiver_account_id": "b.near"
    });
    roots[0]["receipts"] = Value::Array(std::iter::repeat_n(leaf, MAX_RECEIPTS_PER_TX).collect());
    assert!(matches!(
        parse_tx_receipts(&tx_hash, &roots).unwrap_err(),
        TxParseError::TooManyReceipts
    ));
}

#[test]
fn deeply_nested_tree_does_not_overflow_the_stack() {
    // serde_json's recursion limit turns a pathological depth into a
    // Shape error long before the walker runs.
    let mut text = String::new();
    for _ in 0..5_000 {
        text.push_str(
            "{\"receipt_id\":\"x\",\"predecessor_account_id\":\"a.near\",\"receiver_account_id\":\"b.near\",\"receipts\":[",
        );
    }
    text.push_str("{}");
    for _ in 0..5_000 {
        text.push_str("]}");
    }
    let tx_hash = TxHash::new("6LAHAus8voUyKFffiFri6UX6yMPCtsctqRzLzmvyrmYM").unwrap();
    let value = serde_json::from_str::<Value>(&text);
    // Either the JSON layer refuses the depth, or the walker returns an
    // error; both are acceptable, a stack overflow is not.
    if let Ok(v) = value {
        assert!(parse_tx_receipts(&tx_hash, &[v]).is_err());
    }
}
