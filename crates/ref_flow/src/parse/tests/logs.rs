//! `Swapped` log line parsing and msg un-escaping in isolation.

use super::*;

#[test]
fn parses_plain_and_fee_suffixed_lines() {
    let plain =
        parse_swapped_log("Swapped 2991757 zec.omft.near for 7682134962619246593392108 wrap.near")
            .unwrap();
    assert_eq!(plain.amount_in, 2_991_757);
    assert_eq!(plain.token_in, tok("zec.omft.near"));
    assert_eq!(plain.amount_out, 7_682_134_962_619_246_593_392_108);
    assert_eq!(plain.token_out, tok("wrap.near"));

    let with_fee = parse_swapped_log(
        "Swapped 992681317691521195417516 wrap.near for 3027 nbtc.bridge.near, total fee 9, admin fee 1",
    )
    .unwrap();
    assert_eq!(with_fee.amount_out, 3_027);
    assert_eq!(with_fee.token_out, tok("nbtc.bridge.near"));
}

#[test]
fn accepts_u128_max_and_rejects_longer_numbers() {
    let max = u128::MAX.to_string();
    let ok = parse_swapped_log(&format!("Swapped {max} a.near for {max} b.near")).unwrap();
    assert_eq!(ok.amount_in, u128::MAX);
    // 39 digits above u128::MAX, and 40 digits.
    assert!(
        parse_swapped_log("Swapped 340282366920938463463374607431768211456 a.near for 1 b.near")
            .is_err()
    );
    assert!(
        parse_swapped_log("Swapped 1000000000000000000000000000000000000000 a.near for 1 b.near")
            .is_err()
    );
}

#[test]
fn rejects_overlong_lines() {
    let line = format!(
        "Swapped 1 a.near for 1 b.near{}",
        " ".repeat(MAX_LOG_LINE_LEN)
    );
    assert!(matches!(
        parse_swapped_log(&line),
        Err(TradeRejectReason::MalformedLog(_))
    ));
}

#[test]
fn strips_any_number_of_backslashes_before_quotes() {
    assert_eq!(strip_escaped_quotes(r#"{\"a\":1}"#), r#"{"a":1}"#);
    assert_eq!(
        strip_escaped_quotes(r#"{\\\"a\\\":\\\"x\\\"}"#),
        r#"{"a":"x"}"#
    );
    // Backslashes not followed by a quote are preserved.
    assert_eq!(strip_escaped_quotes(r#"a\\b"#), r#"a\\b"#);
    assert_eq!(strip_escaped_quotes(r#"tail\"#), r#"tail\"#);
}

#[test]
fn ft_msg_accepts_plain_escaped_and_string_wrapped_json() {
    let plain = r#"{"force":0,"actions":[{"pool_id":1,"token_in":"a.near","token_out":"b.near","amount_in":"5"}]}"#;
    assert_eq!(parse_ft_msg(plain).unwrap().len(), 1);
    let escaped = r#"{\\\"force\\\":0,\\\"actions\\\":[{\\\"pool_id\\\":1,\\\"token_in\\\":\\\"a.near\\\",\\\"token_out\\\":\\\"b.near\\\"}]}"#;
    assert_eq!(parse_ft_msg(escaped).unwrap()[0].pool_id, 1);
    // JSON string whose content is the JSON object (double encoded).
    let wrapped = serde_json::to_string(plain).unwrap();
    assert_eq!(parse_ft_msg(&wrapped).unwrap().len(), 1);
    let double_wrapped = serde_json::to_string(&wrapped).unwrap();
    assert_eq!(parse_ft_msg(&double_wrapped).unwrap().len(), 1);
    // Three levels is one too many.
    let triple = serde_json::to_string(&double_wrapped).unwrap();
    assert_eq!(
        parse_ft_msg(&triple).unwrap_err(),
        TradeRejectReason::MalformedMsg
    );
}

#[test]
fn ft_msg_without_actions_is_an_empty_list_and_garbage_is_malformed() {
    assert!(
        parse_ft_msg(r#"{"Swap":{"pool_ids":["x"]}}"#)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        parse_ft_msg("not json").unwrap_err(),
        TradeRejectReason::MalformedMsg
    );
    assert_eq!(
        parse_ft_msg("[1,2]").unwrap_err(),
        TradeRejectReason::MalformedMsg
    );
}

#[test]
fn decodes_base64_json_integers_as_string_or_number() {
    assert_eq!(decode_result_int(&b64_json("\"0\"")), Some(0));
    assert_eq!(
        decode_result_int(&b64_json("\"7682134962619246593392108\"")),
        Some(7_682_134_962_619_246_593_392_108)
    );
    assert_eq!(decode_result_int(&b64_json("42")), Some(42));
    assert_eq!(decode_result_int(&b64_json("\"-1\"")), None);
    assert_eq!(decode_result_int(&b64_json("true")), None);
    assert_eq!(decode_result_int("!!not base64!!"), None);
}

#[test]
fn block_timestamp_converts_nanoseconds() {
    let block: Block = serde_json::from_value(serde_json::json!({
        "block_height": "221385535",
        "block_timestamp": "1792843194365844150"
    }))
    .unwrap();
    assert_eq!(block.block_height, 221_385_535);
    let t = block.time().unwrap();
    assert_eq!(t.and_utc().timestamp(), 1_792_843_194);
    assert_eq!(t.and_utc().timestamp_subsec_nanos(), 365_844_150);
    let numeric: Block = serde_json::from_value(serde_json::json!({
        "block_height": 5, "block_timestamp": 1_000_000_000u64
    }))
    .unwrap();
    assert_eq!(numeric.time().unwrap().and_utc().timestamp(), 1);
}
