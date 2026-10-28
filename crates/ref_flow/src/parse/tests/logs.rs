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
fn unescape_level_peels_exactly_one_level() {
    assert_eq!(unescape_level(r#"{\"a\":1}"#).unwrap(), r#"{"a":1}"#);
    assert_eq!(unescape_level(r#"{\\\"a\\\":1}"#).unwrap(), r#"{\"a\":1}"#);
    assert_eq!(unescape_level("plain").unwrap(), "plain");
    assert!(unescape_level("\"unbalanced").is_none());
}

#[test]
fn ft_msg_keeps_nested_json_strings_intact() {
    // Real shape: msg escaped three levels deep with a client_echo field
    // that itself holds a JSON string (one level deeper).
    let msg = r#"{\\\"force\\\":0,\\\"actions\\\":[{\\\"pool_id\\\":5391,\\\"token_in\\\":\\\"kat.token0.near\\\",\\\"token_out\\\":\\\"wrap.near\\\",\\\"amount_in\\\":\\\"4526\\\",\\\"min_amount_out\\\":\\\"229075\\\"}],\\\"client_echo\\\":\\\"{\\\\\\\"receiver_id\\\\\\\":\\\\\\\"solver-ref.near\\\\\\\"}\\\"}"#;
    let actions = parse_ft_msg(msg).unwrap().unwrap();
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].pool_id, 5391);
    assert_eq!(actions[0].token_in, "kat.token0.near");
}

#[test]
fn empty_ft_msg_is_a_deposit_not_an_error() {
    assert!(parse_ft_msg("").unwrap().is_none());
    assert!(parse_ft_msg("   ").unwrap().is_none());
}

#[test]
fn swap_by_output_logs_share_the_swapped_shape() {
    let log = parse_swapped_log(
        "Swap_by_output 6017424610129316358802151 wrap.near for 55000 usdt.tether-token.near",
    )
    .unwrap();
    assert_eq!(log.amount_in, 6_017_424_610_129_316_358_802_151);
    assert_eq!(log.amount_out, 55_000);
}

#[test]
fn ft_msg_accepts_plain_escaped_and_string_wrapped_json() {
    let plain = r#"{"force":0,"actions":[{"pool_id":1,"token_in":"a.near","token_out":"b.near","amount_in":"5"}]}"#;
    assert_eq!(parse_ft_msg(plain).unwrap().unwrap().len(), 1);
    let escaped = r#"{\\\"force\\\":0,\\\"actions\\\":[{\\\"pool_id\\\":1,\\\"token_in\\\":\\\"a.near\\\",\\\"token_out\\\":\\\"b.near\\\"}]}"#;
    assert_eq!(parse_ft_msg(escaped).unwrap().unwrap()[0].pool_id, 1);
    // JSON string whose content is the JSON object (double encoded).
    let wrapped = serde_json::to_string(plain).unwrap();
    assert_eq!(parse_ft_msg(&wrapped).unwrap().unwrap().len(), 1);
    let double_wrapped = serde_json::to_string(&wrapped).unwrap();
    assert_eq!(parse_ft_msg(&double_wrapped).unwrap().unwrap().len(), 1);
    // More levels than MAX_MSG_ESCAPE_LEVELS is one too many.
    let mut text = plain.to_owned();
    for _ in 0..=MAX_MSG_ESCAPE_LEVELS {
        text = serde_json::to_string(&text).unwrap();
    }
    assert_eq!(
        parse_ft_msg(&text).unwrap_err(),
        TradeRejectReason::MalformedMsg
    );
}

#[test]
fn ft_msg_without_actions_is_an_empty_list_and_garbage_is_malformed() {
    assert!(
        parse_ft_msg(r#"{"Swap":{"pool_ids":["x"]}}"#)
            .unwrap()
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

#[test]
fn nanosecond_conversions_round_trip_and_clamp() {
    let ns = 1_792_843_194_365_844_150u64;
    let t = ns_to_time(ns).unwrap();
    assert_eq!(time_to_ns(t), ns);
    assert_eq!(ns_to_time(0).unwrap().and_utc().timestamp(), 0);
    assert_eq!(time_to_ns(NaiveDateTime::MIN), 0, "before the epoch clamps");
}
