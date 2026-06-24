use super::*;
use std::str::FromStr;

const ONE_NEAR_YOCTO: u128 = 1_000_000_000_000_000_000_000_000;

#[test]
fn test_to_cost_deduction_with_basis_zero_held_returns_zero_position_error() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let trade = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let zero = YoctoValue::zero();
    assert_eq!(
        breakdown.to_cost_deduction_with_basis(&trade, &zero),
        Err(CostError::ZeroPosition)
    );
}

#[test]
fn test_to_cost_deduction_with_basis_equal_trade_held_matches_legacy_ratio() {
    // trade_size == held_size のとき、新 API は旧 to_cost_deduction と同じ ratio を返す
    // （Phase 1 互換）。0.01 (variable) + 0.001 (fixed/1) = 0.011。
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction_with_basis(&assumed, &assumed)
        .expect("non-zero held_size");
    let value = deduction.as_f64();
    assert!(value.is_finite() && value >= 0.0);
    assert!((value - 0.011).abs() < 1e-6, "expected 0.011, got {value}");
}

#[test]
fn test_to_cost_deduction_with_basis_rejects_non_finite_variable_ratio() {
    let breakdown = TradeCostBreakdown {
        variable_ratio: f64::NAN,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    assert_eq!(
        breakdown.to_cost_deduction_with_basis(&assumed, &assumed),
        Err(CostError::NonFiniteRatio)
    );
}

#[test]
fn test_to_cost_deduction_with_basis_zero_trade_only_fixed_cost_amortized() {
    // partial entry / hold で trade=0 だが held>0 のケース: variable cost 0、
    // fixed cost のみが held で割られる。production では Δw≈0 なので
    // 0 deduction で短絡されるが、to_cost_deduction_with_basis の単独動作として
    // ZeroPosition でなく fixed-only ratio を返すことを pin。
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000), // 0.001 NEAR
    };
    let trade = YoctoValue::zero();
    let held = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let value = breakdown
        .to_cost_deduction_with_basis(&trade, &held)
        .expect("non-zero held_size")
        .as_f64();
    // var_cost = 0.01 × 0 = 0、fixed = 0.001、deduction = 0.001 / 1.0 = 0.001
    assert!((value - 0.001).abs() < 1e-9, "expected 0.001, got {value}");
}

#[test]
fn test_to_cost_deduction_with_basis_partial_exit_smaller_trade_than_held() {
    // 部分 exit シナリオ: trade=0.4 NEAR, held=0.6 NEAR (target=0.6, current=1.0 等)
    // var_cost = 0.01 × 0.4 = 0.004
    // fixed = 0.001
    // deduction = (0.004 + 0.001) / 0.6 ≈ 0.008333
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let trade = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO * 4 / 10);
    let held = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO * 6 / 10);
    let value = breakdown
        .to_cost_deduction_with_basis(&trade, &held)
        .expect("non-zero held")
        .as_f64();
    let expected = (0.01 * 0.4 + 0.001) / 0.6;
    assert!(
        (value - expected).abs() < 1e-9,
        "expected {expected}, got {value}"
    );
}

#[test]
fn test_cost_deduction_rejects_non_finite() {
    assert!(CostDeduction::new(f64::NAN).is_none());
    assert!(CostDeduction::new(f64::INFINITY).is_none());
    assert!(CostDeduction::new(f64::NEG_INFINITY).is_none());
}

#[test]
fn test_cost_deduction_rejects_negative() {
    assert!(CostDeduction::new(-0.001).is_none());
}

#[test]
fn test_cost_deduction_accepts_zero_and_positive() {
    assert_eq!(
        CostDeduction::new(0.0).map(CostDeduction::as_f64),
        Some(0.0)
    );
    assert_eq!(
        CostDeduction::new(0.5).map(CostDeduction::as_f64),
        Some(0.5)
    );
}

#[test]
fn test_cost_deduction_accepts_value_at_sane_cap() {
    // 上限ちょうどは許容（境界 inclusive）。
    assert_eq!(
        CostDeduction::new(COST_DEDUCTION_SANE_MAX).map(CostDeduction::as_f64),
        Some(COST_DEDUCTION_SANE_MAX)
    );
}

#[test]
fn test_cost_deduction_rejects_above_sane_cap() {
    // typed config bypass / hostile RPC 経由の巨大 finite ratio を遮断。
    let above = COST_DEDUCTION_SANE_MAX * 1.0001;
    assert!(CostDeduction::new(above).is_none());
    // 1e+270 のような subnormal target_w 経由の値も同様に弾かれる。
    assert!(CostDeduction::new(1.0e+270).is_none());
}

#[test]
fn test_to_cost_deduction_with_basis_excessive_ratio_signal() {
    // `held_size = 1e-9 NEAR` 級で variable_ratio + fixed が `held` を桁で
    // 超えるシナリオ: `target_w = 1e-9 × total_value` のような subnormal 経路の
    // モデルテスト。`is_finite()` を満たすが SANE_MAX を超える ratio が
    // ExcessiveRatio として上位に伝わることを pin。
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        // 1 NEAR の固定費（gas + storage で発生し得る現実的オーダー）
        fixed_cost: YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO),
    };
    let trade = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO / 1_000);
    // held = 0.0001 NEAR -> ratio ≈ (0.005 × 0.001 + 1.0) / 0.0001 ≈ 10005 >> 10
    let held = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO / 10_000);
    match breakdown.to_cost_deduction_with_basis(&trade, &held) {
        Err(CostError::ExcessiveRatio { value, cap }) => {
            assert!(value > cap, "ExcessiveRatio must carry value > cap");
            assert_eq!(cap, COST_DEDUCTION_SANE_MAX);
        }
        other => panic!("expected ExcessiveRatio, got {other:?}"),
    }
}

#[test]
fn test_to_cost_deduction_with_basis_combines_variable_and_fixed() {
    // trade == held (entry-from-cash 等価) のとき、ratio = variable + fixed/held
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        // 固定費 0.001 NEAR
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction_with_basis(&assumed, &assumed)
        .expect("non-zero held")
        .as_f64();
    assert!(
        (deduction - 0.011).abs() < 1e-6,
        "expected 0.011, got {deduction}"
    );
}

#[test]
fn test_to_cost_deduction_with_basis_larger_held_reduces_fixed_ratio() {
    // trade と held を同じスケールで増やすと fixed_cost 部分が希釈されて
    // deduction が小さくなる（variable_ratio は held に対する比例で残る）。
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.005,
        fixed_cost: YoctoValue::from_yocto_u128(1_000_000_000_000_000_000_000),
    };
    let small = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let large = YoctoValue::from_yocto_u128(100 * ONE_NEAR_YOCTO);
    let small_d = breakdown
        .to_cost_deduction_with_basis(&small, &small)
        .expect("non-zero held")
        .as_f64();
    let large_d = breakdown
        .to_cost_deduction_with_basis(&large, &large)
        .expect("non-zero held")
        .as_f64();
    assert!(
        large_d < small_d,
        "larger held should reduce fixed-cost share: small={small_d}, large={large_d}"
    );
}

#[test]
fn test_to_cost_deduction_with_basis_only_variable_when_fixed_zero() {
    // fixed_cost = 0 で trade == held のとき deduction == variable_ratio
    let breakdown = TradeCostBreakdown {
        variable_ratio: 0.01,
        fixed_cost: YoctoValue::zero(),
    };
    let assumed = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let deduction = breakdown
        .to_cost_deduction_with_basis(&assumed, &assumed)
        .expect("non-zero held")
        .as_f64();
    assert!(
        (deduction - 0.01).abs() < 1e-9,
        "expected 0.01, got {deduction}"
    );
}

#[test]
fn test_compute_loss_ratio_basic() {
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str("0.99").unwrap());
    let loss = compute_loss_ratio(&input, &output).expect("finite f64 conversion");
    assert!((loss - 0.01).abs() < 1e-9, "expected ~0.01, got {loss}");
}

#[test]
fn test_compute_loss_ratio_zero_input_returns_zero() {
    let input = NearValue::zero();
    let output = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    assert_eq!(
        compute_loss_ratio(&input, &output).expect("zero input is Ok(0)"),
        0.0
    );
}

#[test]
fn test_compute_loss_ratio_micro_negative_silent_zero() {
    // |raw| < LOSS_RATIO_NEGATIVE_WARN_THRESHOLD 域は浮動小数点ノイズとして
    // silent に 0 化（warn しない）。1e-9 級。
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str("1.0000000001").unwrap());
    assert_eq!(
        compute_loss_ratio(&input, &output).expect("noise band returns Ok(0)"),
        0.0
    );
}

#[test]
fn test_compute_loss_ratio_just_inside_warn_band_silent_zero() {
    // raw が `-LOSS_RATIO_NEGATIVE_WARN_THRESHOLD` の絶対値直下（warn しない側）
    // で 0.0 にクランプされること。閾値定数を直接参照して定数 drift 耐性を持たせる。
    let near_threshold = LOSS_RATIO_NEGATIVE_WARN_THRESHOLD * 0.5;
    let output_value = 1.0 + near_threshold;
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str(&format!("{output_value}")).unwrap());
    assert_eq!(
        compute_loss_ratio(&input, &output).expect("noise band returns Ok(0)"),
        0.0
    );
}

#[test]
fn test_compute_loss_ratio_above_warn_threshold_clamps_to_zero() {
    // raw < -LOSS_RATIO_NEGATIVE_WARN_THRESHOLD → warn ログを出すが返り値は
    // 0.0 にクランプ（挙動互換）。smoke test として panic せず 0 を返すこと。
    let above_threshold = LOSS_RATIO_NEGATIVE_WARN_THRESHOLD * 10.0;
    let output_value = 1.0 + above_threshold;
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str(&format!("{output_value}")).unwrap());
    assert_eq!(
        compute_loss_ratio(&input, &output).expect("clamps to Ok(0)"),
        0.0
    );
}

#[test]
fn test_compute_loss_ratio_far_above_warn_threshold_clamps_to_zero() {
    // raw が大きく負（-1e-2 級）でも 0 にクランプ。warn が出るが返り値は変わらない。
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::from_near(BigDecimal::from_str("1.05").unwrap());
    assert_eq!(
        compute_loss_ratio(&input, &output).expect("clamps to Ok(0)"),
        0.0
    );
}

#[test]
fn test_compute_loss_ratio_full_loss() {
    // output = 0 → loss = 100%
    let input = NearValue::from_near(BigDecimal::from_str("1.0").unwrap());
    let output = NearValue::zero();
    let loss = compute_loss_ratio(&input, &output).expect("finite f64 conversion");
    assert!((loss - 1.0).abs() < 1e-12);
}

#[test]
fn test_clamp_storage_min_under_cap_passthrough() {
    // 通常の運用値（0.1 NEAR）はクランプされずそのまま返る
    let normal = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);
    let clamped = clamp_storage_min(&normal);
    assert_eq!(clamped, 100_000_000_000_000_000_000_000);
    assert!(clamped < STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_at_cap_returns_cap() {
    let at_cap = YoctoValue::from_yocto_u128(STORAGE_MIN_SANE_CAP);
    assert_eq!(clamp_storage_min(&at_cap), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_above_cap_clamped() {
    // 100 NEAR — cap (1 NEAR) を超える
    let above = YoctoValue::from_yocto_u128(100 * 10u128.pow(24));
    assert_eq!(clamp_storage_min(&above), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_u128_max_clamped_to_cap() {
    // RPC が u128::MAX を返す敵対的シナリオでも cap で吸収される
    let hostile = YoctoValue::from_yocto_u128(u128::MAX);
    assert_eq!(clamp_storage_min(&hostile), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_overflow_u128_clamped_to_cap() {
    // BigDecimal が u128 に収まらない値（u128::MAX + 1）でも cap にクランプ
    let huge = YoctoValue::from_yocto(BigDecimal::from(u128::MAX) + BigDecimal::from(1));
    assert_eq!(clamp_storage_min(&huge), STORAGE_MIN_SANE_CAP);
}

#[test]
fn test_clamp_storage_min_saturating_mul_safe_with_max_token_count() {
    // クランプされた値 × 最大 token 数（MAX_NEW_TOKEN_COUNT = 16）が
    // u128 範囲内に収まることを確認（overflow せず saturate もしない）。
    let hostile = YoctoValue::from_yocto_u128(u128::MAX);
    let clamped = clamp_storage_min(&hostile);
    let mul = clamped.checked_mul(MAX_NEW_TOKEN_COUNT as u128);
    assert!(
        mul.is_some(),
        "STORAGE_MIN_SANE_CAP × MAX_NEW_TOKEN_COUNT must not overflow u128"
    );
}

#[test]
fn test_estimate_trade_cost_at_max_new_token_count_succeeds() {
    // 境界値: new_token_count = MAX_NEW_TOKEN_COUNT (= 16) は通過する
    use blockchain::types::gas_price::GasPrice;
    use dex::TokenPath;
    use near_sdk::NearToken;

    let buy_path = TokenPath(vec![]);
    let sell_path = TokenPath(vec![]);
    let assumed_in = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let spot_rate = ExchangeRate::wnear();
    let gas_price = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));
    let storage_min = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);

    let result = estimate_trade_cost(
        &buy_path,
        &sell_path,
        &assumed_in,
        &spot_rate,
        gas_price,
        &storage_min,
        MAX_NEW_TOKEN_COUNT,
    );
    assert!(
        result.is_ok(),
        "MAX_NEW_TOKEN_COUNT must be accepted, got {:?}",
        result.err()
    );
}

#[test]
fn test_estimate_trade_cost_one_hop_round_trip_includes_amm_loss() {
    // 1-hop round-trip: BUY (wnear → token_x) + SELL (token_x → wnear) を
    // それぞれ通すと、variable_ratio に AMM fee + price impact が両方向ぶん
    // 載って `2 × EXPECTED_SLIPPAGE_DEDUCTION` を上回ること。
    //
    // プール構成: wnear / token_x, 100_000 wnear : 1_000 token_x
    //   → 1 token_x = 100 NEAR (price 100, 両方 24 decimals)
    //   → fee = 30 / 10000 = 0.3%
    //
    // trade_size = 1 NEAR (1e24 yocto):
    //   - BUY 入力 1 NEAR → 出力 ≈ 0.00997 token_x → NEAR 換算 ≈ 0.997 NEAR
    //     → loss ≈ 0.3% (fee dominant)
    //   - SELL 入力 1 NEAR worth = 0.01 token_x → 出力 ≈ 0.997 NEAR
    //     → loss ≈ 0.3%
    // 合算 ≈ 0.6% + 2×0.5% slippage = ~1.6%
    use bigdecimal::BigDecimal;
    use blockchain::types::gas_price::GasPrice;
    use chrono::Utc;
    use dex::{PoolInfo, PoolInfoBared, TokenIn, TokenOut, TokenPath};
    use near_sdk::NearToken;
    use near_sdk::json_types::U128;
    use std::sync::Arc;

    // wnear (token index 0) と token_x (token index 1)、両方 24 decimals
    let pool = Arc::new(PoolInfo::new(
        0,
        PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: vec![
                "wrap.near".parse().unwrap(),
                "token_x.near".parse().unwrap(),
            ],
            // 100_000 wnear : 1_000 token_x
            amounts: vec![U128(100_000 * ONE_NEAR_YOCTO), U128(1_000 * ONE_NEAR_YOCTO)],
            total_fee: 30,
            shares_total_supply: U128(0),
            amp: 0,
        },
        Utc::now().naive_utc(),
    ));
    // BUY: wnear (in=0) → token_x (out=1)
    let buy_pair = pool
        .get_pair(TokenIn::from(0), TokenOut::from(1))
        .expect("valid buy pair");
    let buy_path = TokenPath(vec![buy_pair]);
    // SELL: token_x (in=1) → wnear (out=0)
    let sell_pair = pool
        .get_pair(TokenIn::from(1), TokenOut::from(0))
        .expect("valid sell pair");
    let sell_path = TokenPath(vec![sell_pair]);

    // trade_size: 1 NEAR
    let trade_size = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    // spot_rate: 1 token_x = 100 NEAR → ExchangeRate(raw_rate = 1e24/100, decimals=24)
    let spot_rate = ExchangeRate::from_raw_rate(BigDecimal::from(ONE_NEAR_YOCTO / 100), 24);
    let gas_price = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));
    let storage_min = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);

    let breakdown = estimate_trade_cost(
        &buy_path,
        &sell_path,
        &trade_size,
        &spot_rate,
        gas_price,
        &storage_min,
        1,
    )
    .expect("end-to-end round-trip AMM path must succeed");

    // variable_ratio は 2 × EXPECTED_SLIPPAGE_DEDUCTION (= 0.01) を上回る
    // 必要がある（両方向で fee + 微小 price impact が乗るため）
    assert!(
        breakdown.variable_ratio > 2.0 * EXPECTED_SLIPPAGE_DEDUCTION,
        "round-trip AMM loss must exceed 2x slippage budget; got {}",
        breakdown.variable_ratio
    );
    // 上限のサニティ: 1 NEAR / 100_000 NEAR pool ≈ 0.001% price impact、
    // 両方向で fee 0.3% × 2 + 各方向の slippage 0.5% × 2 = ~1.6% で
    // 5% を大きく超えないこと
    assert!(
        breakdown.variable_ratio < 0.05,
        "round-trip AMM loss should not exceed 5% for a 0.001% pool fraction; got {}",
        breakdown.variable_ratio
    );
}

#[test]
fn test_round_trip_cost_ratio_smart_constructor_rejects_non_finite_and_excessive() {
    // Newtype の不変条件: is_finite() && 0.0 <= value <= COST_DEDUCTION_SANE_MAX (= 10.0)
    assert!(RoundTripCostRatio::new(f64::NAN).is_none());
    assert!(RoundTripCostRatio::new(f64::INFINITY).is_none());
    assert!(RoundTripCostRatio::new(f64::NEG_INFINITY).is_none());
    assert!(RoundTripCostRatio::new(-0.001).is_none());
    assert!(RoundTripCostRatio::new(COST_DEDUCTION_SANE_MAX + 0.001).is_none());

    // 境界値（0.0、上限）は受け入れる
    assert_eq!(RoundTripCostRatio::new(0.0).map(|r| r.as_f64()), Some(0.0));
    assert_eq!(
        RoundTripCostRatio::new(COST_DEDUCTION_SANE_MAX).map(|r| r.as_f64()),
        Some(COST_DEDUCTION_SANE_MAX)
    );
    assert_eq!(RoundTripCostRatio::new(0.5).map(|r| r.as_f64()), Some(0.5));
}

#[test]
fn test_estimate_full_position_round_trip_ratio_zero_position_bails() {
    use blockchain::types::gas_price::GasPrice;
    use dex::TokenPath;
    use near_sdk::NearToken;

    let buy_path = TokenPath(vec![]);
    let sell_path = TokenPath(vec![]);
    let zero = YoctoValue::zero();
    let spot_rate = ExchangeRate::wnear();
    let gas_price = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));
    let storage_min = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);

    let err = estimate_full_position_round_trip_ratio(
        &buy_path,
        &sell_path,
        &zero,
        &spot_rate,
        gas_price,
        &storage_min,
        0,
    )
    .expect_err("position_size = 0 must bail");
    assert!(format!("{err}").contains("position_size"));
}

#[test]
fn test_estimate_full_position_round_trip_ratio_one_hop_matches_breakdown() {
    // estimate_trade_cost と一致した上で、fixed_cost / position_size を加えた
    // ratio を返すこと。1 NEAR / 100_000 NEAR プール、両 24 decimals。
    use bigdecimal::BigDecimal;
    use blockchain::types::gas_price::GasPrice;
    use chrono::Utc;
    use dex::{PoolInfo, PoolInfoBared, TokenIn, TokenOut, TokenPath};
    use near_sdk::NearToken;
    use near_sdk::json_types::U128;
    use std::sync::Arc;

    let pool = Arc::new(PoolInfo::new(
        0,
        PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: vec![
                "wrap.near".parse().unwrap(),
                "token_x.near".parse().unwrap(),
            ],
            amounts: vec![U128(100_000 * ONE_NEAR_YOCTO), U128(1_000 * ONE_NEAR_YOCTO)],
            total_fee: 30,
            shares_total_supply: U128(0),
            amp: 0,
        },
        Utc::now().naive_utc(),
    ));
    let buy_pair = pool
        .get_pair(TokenIn::from(0), TokenOut::from(1))
        .expect("valid buy pair");
    let sell_pair = pool
        .get_pair(TokenIn::from(1), TokenOut::from(0))
        .expect("valid sell pair");
    let buy_path = TokenPath(vec![buy_pair]);
    let sell_path = TokenPath(vec![sell_pair]);

    let position_size = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let spot_rate = ExchangeRate::from_raw_rate(BigDecimal::from(ONE_NEAR_YOCTO / 100), 24);
    let gas_price = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));
    let storage_min = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);

    let breakdown = estimate_trade_cost(
        &buy_path,
        &sell_path,
        &position_size,
        &spot_rate,
        gas_price,
        &storage_min,
        1,
    )
    .expect("breakdown reference");
    let expected_ratio = breakdown.variable_ratio
        + breakdown
            .fixed_cost
            .to_near()
            .as_bigdecimal()
            .to_f64()
            .unwrap()
            / position_size.to_near().as_bigdecimal().to_f64().unwrap();

    let ratio = estimate_full_position_round_trip_ratio(
        &buy_path,
        &sell_path,
        &position_size,
        &spot_rate,
        gas_price,
        &storage_min,
        1,
    )
    .expect("end-to-end ratio must succeed");
    assert!(
        (ratio.as_f64() - expected_ratio).abs() < 1e-9,
        "expected {expected_ratio}, got {}",
        ratio.as_f64()
    );
    // Round-trip cost should exceed 2× EXPECTED_SLIPPAGE_DEDUCTION (= 0.01)
    // because both legs incur fee + price impact.
    assert!(ratio.as_f64() > 2.0 * EXPECTED_SLIPPAGE_DEDUCTION);
}

#[test]
fn test_estimate_trade_cost_above_max_new_token_count_bails() {
    // 境界値: new_token_count > MAX_NEW_TOKEN_COUNT (17) は bail! で除外される
    use blockchain::types::gas_price::GasPrice;
    use dex::TokenPath;
    use near_sdk::NearToken;

    let buy_path = TokenPath(vec![]);
    let sell_path = TokenPath(vec![]);
    let assumed_in = YoctoValue::from_yocto_u128(ONE_NEAR_YOCTO);
    let spot_rate = ExchangeRate::wnear();
    let gas_price = GasPrice::from_balance(NearToken::from_yoctonear(100_000_000));
    let storage_min = YoctoValue::from_yocto_u128(100_000_000_000_000_000_000_000);

    let err = estimate_trade_cost(
        &buy_path,
        &sell_path,
        &assumed_in,
        &spot_rate,
        gas_price,
        &storage_min,
        MAX_NEW_TOKEN_COUNT + 1,
    )
    .expect_err("MAX_NEW_TOKEN_COUNT + 1 must be rejected");
    let msg = format!("{err}");
    assert!(
        msg.contains("MAX_NEW_TOKEN_COUNT"),
        "expected MAX_NEW_TOKEN_COUNT in error, got: {msg}"
    );
}
