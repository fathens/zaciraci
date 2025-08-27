use super::*;
use bigdecimal::FromPrimitive;
use chrono::Duration;

#[test]
fn test_calculate_trend_strength() {
    let prices = vec![100.0, 102.0, 105.0, 108.0, 112.0];
    let timestamps = vec![
        Utc::now(),
        Utc::now() + Duration::hours(1),
        Utc::now() + Duration::hours(2),
        Utc::now() + Duration::hours(3),
        Utc::now() + Duration::hours(4),
    ];

    let (slope, r_squared, direction, _strength) = calculate_trend_strength(&prices, &timestamps);

    assert!(slope > 0.0); // 上昇トレンド
    assert!(r_squared > 0.0); // 相関がある
    assert_eq!(direction, TrendDirection::Upward);

    // 強いトレンドのテスト（R²が高い場合）
    let strong_prices = vec![100.0, 105.0, 110.0, 115.0, 120.0];
    let (_, r_squared, _, strength) = calculate_trend_strength(&strong_prices, &timestamps);
    assert!(r_squared > 0.9);
    assert_eq!(strength, TrendStrength::Strong);
}

#[test]
fn test_calculate_rsi() {
    // RSI計算のテスト
    let prices = vec![
        44.0, 44.25, 44.5, 43.75, 44.5, 45.0, 45.25, 45.5, 45.75, 46.0, 46.25, 46.5, 46.75, 47.0,
        47.25,
    ];

    let rsi = calculate_rsi(&prices, 14);
    assert!(rsi.is_some());
    let rsi_value = rsi.unwrap();
    assert!((0.0..=100.0).contains(&rsi_value));

    // 上昇トレンドではRSIが高めになることを確認
    assert!(rsi_value > 50.0);

    // 不十分なデータのテスト
    let short_prices = vec![100.0, 101.0];
    assert!(calculate_rsi(&short_prices, 14).is_none());
}

#[test]
fn test_calculate_macd() {
    let prices = vec![
        22.27, 22.19, 22.08, 22.17, 22.18, 22.13, 22.23, 22.43, 22.24, 22.29, 22.15, 22.39, 22.38,
        22.61, 23.36, 24.05, 23.75, 23.83, 23.95, 23.63, 23.82, 23.87, 23.65, 23.19, 23.10, 23.33,
        22.68, 23.10, 22.40, 22.17,
    ];

    let (macd, signal) = calculate_macd(&prices, 12, 26, 9);

    assert!(macd.is_some());
    assert!(signal.is_some());

    let macd_value = macd.unwrap();
    let signal_value = signal.unwrap();

    // MACDとシグナル線は合理的な範囲内であることを確認
    assert!(macd_value.abs() < 10.0);
    assert!(signal_value.abs() < 10.0);

    // 不十分なデータのテスト
    let short_prices = vec![100.0, 101.0, 102.0];
    let (macd, signal) = calculate_macd(&short_prices, 12, 26, 9);
    assert!(macd.is_none());
    assert!(signal.is_none());
}

#[test]
fn test_calculate_adx() {
    let highs = vec![
        48.7, 48.72, 48.9, 48.87, 48.82, 49.05, 49.2, 49.35, 49.92, 50.19,
    ];
    let lows = vec![
        47.79, 48.14, 48.39, 48.37, 48.24, 48.64, 48.94, 49.1, 49.5, 49.87,
    ];
    let closes = vec![
        48.16, 48.61, 48.75, 48.63, 48.74, 49.03, 49.07, 49.32, 49.91, 50.13,
    ];

    let adx = calculate_adx(&highs, &lows, &closes, 5);

    assert!(adx.is_some());
    let adx_value = adx.unwrap();
    assert!((0.0..=100.0).contains(&adx_value));

    // 不十分なデータのテスト
    let short_highs = vec![100.0, 101.0];
    let short_lows = vec![99.0, 100.0];
    let short_closes = vec![99.5, 100.5];
    assert!(calculate_adx(&short_highs, &short_lows, &short_closes, 14).is_none());

    // 不整合データのテスト
    let mismatched_lows = vec![99.0];
    assert!(calculate_adx(&highs, &mismatched_lows, &closes, 5).is_none());
}

#[test]
fn test_analyze_volume_trend() {
    // 正の相関（価格上昇時にボリューム増加）
    let prices = vec![100.0, 105.0, 110.0, 115.0];
    let volumes = vec![1000.0, 1200.0, 1500.0, 1800.0];

    let correlation = analyze_volume_trend(&volumes, &prices);
    assert!(correlation > 0.0);

    // 負の相関（価格上昇時にボリューム減少）
    let negative_volumes = vec![1800.0, 1500.0, 1200.0, 1000.0];
    let neg_correlation = analyze_volume_trend(&negative_volumes, &prices);
    assert!(neg_correlation < 0.0);

    // エッジケース
    assert_eq!(analyze_volume_trend(&[], &[]), 0.0);
    assert_eq!(analyze_volume_trend(&[100.0], &[100.0]), 0.0);
}

#[test]
fn test_detect_breakout() {
    // ブレイクアウト成功のケース
    let breakout = detect_breakout(105.0, 100.0, 90.0, 2000.0, 1000.0);
    assert!(breakout);

    // ボリューム不足でブレイクアウト失敗
    let no_volume_breakout = detect_breakout(105.0, 100.0, 90.0, 1000.0, 1000.0);
    assert!(!no_volume_breakout);

    // 価格ブレイクアウトなし
    let no_price_breakout = detect_breakout(95.0, 100.0, 90.0, 2000.0, 1000.0);
    assert!(!no_price_breakout);

    // サポート下抜けのブレイクアウト
    let support_breakout = detect_breakout(85.0, 100.0, 90.0, 2000.0, 1000.0);
    assert!(support_breakout);
}

#[test]
fn test_calculate_kelly_position_size() {
    // 通常のケース
    let kelly_size = calculate_kelly_position_size(0.6, 0.15, 0.08, 0.25);
    assert!(kelly_size > 0.0 && kelly_size <= MAX_POSITION_SIZE);

    // 勝率が低い場合
    let low_win_rate = calculate_kelly_position_size(0.3, 0.15, 0.08, 0.25);
    assert!(low_win_rate < kelly_size);

    // エッジケース
    assert_eq!(calculate_kelly_position_size(0.0, 0.15, 0.08, 0.25), 0.0);
    assert_eq!(calculate_kelly_position_size(1.0, 0.15, 0.08, 0.25), 0.0);
    assert_eq!(calculate_kelly_position_size(0.6, 0.15, 0.0, 0.25), 0.0);
}

#[test]
fn test_analyze_trend() {
    let token = "TEST_TOKEN";
    let prices = vec![100.0, 102.0, 105.0, 108.0, 112.0, 115.0];
    let timestamps = vec![
        Utc::now(),
        Utc::now() + Duration::hours(1),
        Utc::now() + Duration::hours(2),
        Utc::now() + Duration::hours(3),
        Utc::now() + Duration::hours(4),
        Utc::now() + Duration::hours(5),
    ];
    let volumes = vec![1000.0, 1100.0, 1300.0, 1200.0, 1400.0, 1500.0];
    let highs = vec![101.0, 103.0, 106.0, 109.0, 113.0, 116.0];
    let lows = vec![99.0, 101.0, 104.0, 107.0, 111.0, 114.0];

    let analysis = analyze_trend(token, &prices, &timestamps, &volumes, &highs, &lows);

    assert_eq!(analysis.token, token);
    assert_eq!(analysis.direction, TrendDirection::Upward);
    assert!(analysis.slope > 0.0);
    assert!(analysis.r_squared > 0.0);
    assert!(analysis.volume_trend >= -1.0 && analysis.volume_trend <= 1.0);
}

#[test]
fn test_make_trend_trading_decision() {
    let strong_upward_analysis = TrendAnalysis {
        token: "TEST".to_string(),
        direction: TrendDirection::Upward,
        strength: TrendStrength::Strong,
        slope: 0.05,
        r_squared: 0.85,
        volume_trend: 0.7,
        breakout_signal: true,
        timestamp: Utc::now(),
    };

    let current_positions = vec![];
    let available_capital = 1000.0;

    // 強いトレンドでブレイクアウトの場合はエントリー
    let action = make_trend_trading_decision(
        &strong_upward_analysis,
        &current_positions,
        available_capital,
    );
    assert!(matches!(action, TrendTradingAction::EnterTrend { .. }));

    // 弱いトレンドの場合は待機
    let weak_analysis = TrendAnalysis {
        strength: TrendStrength::Weak,
        ..strong_upward_analysis.clone()
    };
    let action = make_trend_trading_decision(&weak_analysis, &current_positions, available_capital);
    assert_eq!(action, TrendTradingAction::Wait);

    // サイドウェイトレンドの場合も待機
    let sideways_analysis = TrendAnalysis {
        direction: TrendDirection::Sideways,
        ..strong_upward_analysis
    };
    let action =
        make_trend_trading_decision(&sideways_analysis, &current_positions, available_capital);
    assert_eq!(action, TrendTradingAction::Wait);
}

#[test]
fn test_trend_trading_decision_with_existing_position() {
    let analysis = TrendAnalysis {
        token: "TEST".to_string(),
        direction: TrendDirection::Upward,
        strength: TrendStrength::Strong,
        slope: 0.05,
        r_squared: 0.85,
        volume_trend: 0.7,
        breakout_signal: true,
        timestamp: Utc::now(),
    };

    let existing_position = TrendPosition {
        token: "TEST".to_string(),
        size: 0.1,
        entry_price: BigDecimal::from_f64(100.0).unwrap(),
        entry_time: Utc::now() - Duration::hours(2),
        current_price: BigDecimal::from_f64(110.0).unwrap(),
        unrealized_pnl: 0.1,
    };

    let current_positions = vec![existing_position];
    let available_capital = 0.0;

    // 既存ポジションがある場合は調整を検討
    let action = make_trend_trading_decision(&analysis, &current_positions, available_capital);
    assert!(matches!(action, TrendTradingAction::AdjustPosition { .. }));
}

#[test]
fn test_trend_trading_decision_exit_conditions() {
    let weak_trend_analysis = TrendAnalysis {
        token: "TEST".to_string(),
        direction: TrendDirection::Sideways,
        strength: TrendStrength::Weak,
        slope: 0.001,
        r_squared: 0.2,
        volume_trend: 0.1,
        breakout_signal: false,
        timestamp: Utc::now(),
    };

    let existing_position = TrendPosition {
        token: "TEST".to_string(),
        size: 0.2,
        entry_price: BigDecimal::from_f64(100.0).unwrap(),
        entry_time: Utc::now() - Duration::hours(5),
        current_price: BigDecimal::from_f64(98.0).unwrap(),
        unrealized_pnl: -0.02,
    };

    let current_positions = vec![existing_position];

    // 弱いトレンドの場合は退出
    let action = make_trend_trading_decision(&weak_trend_analysis, &current_positions, 1000.0);
    assert!(matches!(action, TrendTradingAction::ExitTrend { .. }));
}

// ==================== エッジケーステスト ====================

#[test]
fn test_calculate_trend_strength_edge_cases() {
    // 空のデータ
    let empty_prices = vec![];
    let empty_timestamps = vec![];
    let (slope, r_squared, direction, strength) =
        calculate_trend_strength(&empty_prices, &empty_timestamps);
    assert_eq!(slope, 0.0);
    assert_eq!(r_squared, 0.0);
    assert_eq!(direction, TrendDirection::Sideways);
    assert_eq!(strength, TrendStrength::NoTrend);

    // 不整合データ
    let prices = vec![100.0, 101.0];
    let timestamps = vec![Utc::now()]; // 長さが違う
    let (slope, _r_squared, _direction, strength) = calculate_trend_strength(&prices, &timestamps);
    assert_eq!(slope, 0.0);
    assert_eq!(strength, TrendStrength::NoTrend);

    // 同じ価格データ（フラットライン）
    let flat_prices = vec![100.0, 100.0, 100.0, 100.0];
    let timestamps = vec![
        Utc::now(),
        Utc::now() + Duration::hours(1),
        Utc::now() + Duration::hours(2),
        Utc::now() + Duration::hours(3),
    ];
    let (slope, _, direction, _strength) = calculate_trend_strength(&flat_prices, &timestamps);
    assert!(slope.abs() < 0.001);
    assert_eq!(direction, TrendDirection::Sideways);
}

#[test]
fn test_execute_trend_following_strategy() {
    let tokens = vec!["TOKEN1".to_string(), "TOKEN2".to_string()];
    let current_positions = vec![];
    let available_capital = 10000.0;

    // モックデータ
    let mut market_data: HashMap<String, MarketDataTuple> = HashMap::new();

    // TOKEN1: 強い上昇トレンド
    let prices1 = vec![100.0, 105.0, 110.0, 115.0, 120.0];
    let timestamps1 = (0..5).map(|i| Utc::now() + Duration::hours(i)).collect();
    let volumes1 = vec![1000.0, 1200.0, 1500.0, 1800.0, 2000.0];
    let highs1 = vec![101.0, 106.0, 111.0, 116.0, 121.0];
    let lows1 = vec![99.0, 104.0, 109.0, 114.0, 119.0];
    market_data.insert(
        "TOKEN1".to_string(),
        (prices1, timestamps1, volumes1, highs1, lows1),
    );

    // TOKEN2: 弱いトレンド
    let prices2 = vec![200.0, 199.0, 201.0, 200.5, 200.2];
    let timestamps2 = (0..5).map(|i| Utc::now() + Duration::hours(i)).collect();
    let volumes2 = vec![500.0, 520.0, 480.0, 510.0, 495.0];
    let highs2 = vec![201.0, 200.0, 202.0, 201.5, 201.0];
    let lows2 = vec![199.0, 198.0, 200.0, 199.5, 199.5];
    market_data.insert(
        "TOKEN2".to_string(),
        (prices2, timestamps2, volumes2, highs2, lows2),
    );

    // 非同期関数のテストは同期的にテスト
    let rt = tokio::runtime::Runtime::new().unwrap();
    let report = rt
        .block_on(async {
            execute_trend_following_strategy(
                tokens,
                current_positions,
                available_capital,
                &market_data,
            )
            .await
        })
        .unwrap();

    assert_eq!(report.trend_analysis.len(), 2);
    assert_eq!(report.total_signals, 2);
    assert!(report.actions.len() <= 2); // 最大2つのアクション
}

#[test]
fn test_volume_trend_edge_cases() {
    // 同じ長さでない配列
    let prices = vec![100.0, 101.0, 102.0];
    let volumes = vec![1000.0, 1100.0]; // 長さが違う
    assert_eq!(analyze_volume_trend(&volumes, &prices), 0.0);

    // 変化がない場合
    let flat_prices = vec![100.0, 100.0, 100.0];
    let flat_volumes = vec![1000.0, 1000.0, 1000.0];
    assert_eq!(analyze_volume_trend(&flat_volumes, &flat_prices), 0.0);

    // 一つの要素のみ
    let single_price = vec![100.0];
    let single_volume = vec![1000.0];
    assert_eq!(analyze_volume_trend(&single_volume, &single_price), 0.0);
}

#[test]
fn test_trading_constants() {
    // 定数が利用可能であることを確認（コンパイル時チェック）
    let _rsi_test = RSI_OVERBOUGHT - RSI_OVERSOLD; // 正の値になるはず
    let _volume_test = VOLUME_BREAKOUT_MULTIPLIER - 1.0; // 正の値になるはず
    let _r2_test = R_SQUARED_THRESHOLD * 100.0; // パーセンテージ変換
    let _pos_test = MAX_POSITION_SIZE * 1000.0; // スケール変換
    let _kelly_test = KELLY_RISK_FACTOR * 4.0; // スケール変換

    // 実際の値が使用可能であることを実行時に確認
    assert!(_rsi_test > 0.0);
    assert!(_volume_test > 0.0);
    assert!(_r2_test > 0.0);
    assert!(_pos_test > 0.0);
    assert!(_kelly_test > 0.0);
}
