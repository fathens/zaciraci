//! Real receipt trees, one per route shape the collector has to handle.

use super::*;

fn only_trade(parsed: &ParsedTx) -> &Trade {
    assert_eq!(parsed.rejects, vec![], "unexpected rejects");
    assert_eq!(parsed.trades.len(), 1, "expected exactly one trade");
    &parsed.trades[0]
}

#[test]
fn direct_single_hop_swap() {
    let parsed = parse_fixture("swap_single");
    let t = only_trade(&parsed);
    assert_eq!(t.route, TradeRoute::Swap);
    assert_eq!(t.sender.as_str(), "zorro7.near");
    assert_eq!(
        t.receipt_id.as_str(),
        "CFq8ZB7GBJzvY4kegiJbHmrc38isfE4mudZyEqRpQSWG"
    );
    assert_eq!(
        t.tx_hash.as_str(),
        "6LAHAus8voUyKFffiFri6UX6yMPCtsctqRzLzmvyrmYM"
    );
    assert_eq!(t.hops().len(), 1);
    let hop = &t.hops()[0];
    assert_eq!(hop.pool_id, 6065);
    assert_eq!(hop.token_in.0, tok("zec.omft.near"));
    assert_eq!(hop.token_out.0, tok("wrap.near"));
    assert_eq!(hop.amount_in, units(2_991_757));
    assert_eq!(hop.amount_out, units(7_682_134_962_619_246_593_392_108));
    assert_eq!(t.amount_in(), &units(2_991_757));
    assert_eq!(t.amount_out(), &units(7_682_134_962_619_246_593_392_108));
    assert_eq!(t.shape(), RouteShape::Chain);
    assert_eq!(t.fees(), None);
    assert_eq!(t.block_height, 221_289_673);
    assert_eq!(parsed.stats.accepted, 1);
    assert_eq!(parsed.stats.ref_receipts, 1);
}

#[test]
fn two_hop_chain_with_fee_suffix_in_log() {
    let parsed = parse_fixture("swap_chain");
    let t = only_trade(&parsed);
    assert_eq!(t.hops().len(), 2);
    assert_eq!(t.shape(), RouteShape::Chain);
    // First log carries ", total fee 9, admin fee 1".
    assert_eq!(
        t.hops()[0].amount_in,
        units(992_681_317_691_521_195_417_516)
    );
    assert_eq!(t.hops()[0].amount_out, units(3_027));
    assert_eq!(t.hops()[1].pool_id, 6737);
    assert_eq!(t.hops()[1].amount_in, units(3_027));
    assert_eq!(t.amount_in(), &units(992_681_317_691_521_195_417_516));
    assert_eq!(t.amount_out(), &units(994_710_538_388_871_180_240_341));
    assert_eq!(t.token_in().0, tok("wrap.near"));
    assert_eq!(t.token_out().0, tok("wrap.near"));
}

#[test]
fn split_route_sums_parallel_legs() {
    let parsed = parse_fixture("split");
    let t = only_trade(&parsed);
    assert_eq!(t.shape(), RouteShape::Split);
    assert_eq!(t.hops()[0].pool_id, 5577);
    assert_eq!(t.hops()[1].pool_id, 6656);
    assert_eq!(
        t.amount_in(),
        &units(203_872_230_151_579_303_160_053 + 4_160_657_758_195_495_982_858)
    );
    assert_eq!(
        t.amount_out(),
        &units(4_850_784_170_043_114_101_081_843 + 106_210_223_432_896_806_195_420)
    );
}

#[test]
fn swap_plus_withdraw_batch_uses_the_log_not_the_result() {
    // outcome.result belongs to the withdraw action; the Swapped log is
    // the only source of the executed output.
    let parsed = parse_fixture("swap_withdraw");
    let t = only_trade(&parsed);
    assert_eq!(t.route, TradeRoute::Swap);
    assert_eq!(t.sender.as_str(), "intearbots.near");
    assert_eq!(t.hops().len(), 1);
    assert_eq!(t.amount_in(), &units(888_651_470_000_000_000_000_000));
    assert_eq!(t.amount_out(), &units(329_495));
}

#[test]
fn ft_on_transfer_uses_the_transfer_sender_and_msg_actions() {
    let parsed = parse_fixture("ft_basic");
    let t = only_trade(&parsed);
    assert_eq!(t.route, TradeRoute::Ft);
    assert_eq!(t.sender.as_str(), "nabanya1.tg");
    assert_eq!(t.hops()[0].pool_id, 3879);
    assert_eq!(t.amount_in(), &units(10_000_000_000_000_000));
    // The contract reported a zero output for this dust swap.
    assert_eq!(t.amount_out(), &units(0));
    assert_eq!(parsed.stats.ref_receipts, 1);
}

#[test]
fn ft_route_unwrapping_near_has_no_ft_transfer_but_is_still_a_trade() {
    let parsed = parse_fixture("ft_near_withdraw");
    let t = only_trade(&parsed);
    assert_eq!(t.route, TradeRoute::Ft);
    assert_eq!(t.sender.as_str(), "nopanx.near");
    assert_eq!(t.token_in().0, tok("zec.omft.near"));
    assert_eq!(t.token_out().0, tok("wrap.near"));
    assert_eq!(t.amount_in(), &units(7_696_644));
    assert_eq!(t.amount_out(), &units(19_402_086_137_275_989_157_888_371));
}

#[test]
fn ft_split_route_sums_both_legs() {
    let parsed = parse_fixture("ft_split");
    let t = only_trade(&parsed);
    assert_eq!(t.route, TradeRoute::Ft);
    assert_eq!(t.shape(), RouteShape::Split);
    assert_eq!(t.amount_in(), &units(2 * 2_068_012_800_000_000_000_000_000));
    assert_eq!(
        t.amount_out(),
        &units(348_602_050_607_504_272_998 + 348_583_993_999_237_306_126)
    );
}

#[test]
fn failed_ref_receipts_are_skipped_but_the_successful_one_is_kept() {
    let parsed = parse_fixture("failed");
    assert_eq!(parsed.rejects, vec![]);
    assert_eq!(parsed.trades.len(), 1);
    let t = &parsed.trades[0];
    assert_eq!(t.route, TradeRoute::Ft);
    assert_eq!(t.sender.as_str(), "cosmosstable.near");
    assert_eq!(t.amount_in(), &units(20_000_000));
    assert_eq!(t.amount_out(), &units(20_002_212));
    // Three receipts address the Ref contract: the successful
    // ft_on_transfer, a failed one, and Ref's own withdraw callback.
    assert_eq!(parsed.stats.ref_receipts, 3);
    assert_eq!(parsed.stats.failed_receipts, 1);
    assert_eq!(parsed.stats.other_receipts, 1);
    assert_eq!(parsed.stats.accepted, 1);
}

#[test]
fn truncated_msg_is_rejected_not_guessed() {
    let parsed = parse_fixture("ft_unparseable_msg");
    assert!(parsed.trades.is_empty());
    assert_eq!(parsed.rejects.len(), 1);
    assert_eq!(parsed.rejects[0].reason, TradeRejectReason::MalformedMsg);
    assert_eq!(parsed.stats.rejects.get("malformed_msg"), Some(&1));
}

#[test]
fn duplicate_roots_count_as_duplicates_not_trades() {
    let (tx_hash, mut roots) = enrich_fixture("swap_single");
    roots.push(roots[0].clone());
    let parsed = parse_tx_receipts(&tx_hash, &roots).unwrap();
    assert_eq!(parsed.trades.len(), 1);
    assert_eq!(parsed.stats.duplicate_receipts, 1);
    assert_eq!(parsed.stats.ref_receipts, 2);
}

#[test]
fn stats_merge_and_serialise() {
    let mut a = parse_fixture("failed").stats;
    let b = parse_fixture("ft_unparseable_msg").stats;
    a.merge(&b);
    assert_eq!(
        a.ref_receipts, 5,
        "3 + ft_on_transfer + its withdraw callback"
    );
    assert_eq!(a.other_receipts, 2);
    assert_eq!(a.failed_receipts, 1);
    assert_eq!(a.accepted, 1);
    assert_eq!(a.rejects.get("malformed_msg"), Some(&1));
    let json = a.to_json();
    assert_eq!(json["accepted"], 1);
    assert_eq!(json["rejects"]["malformed_msg"], 1);
}
