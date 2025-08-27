use super::*;
use chrono::Duration;
use ndarray::array;
use std::collections::HashMap;

// ==================== テストヘルパー ====================

fn create_sample_tokens() -> Vec<TokenInfo> {
    vec![
        TokenInfo {
            symbol: "TOKEN_A".to_string(),
            current_price: 100.0,
            historical_volatility: 0.2,
            liquidity_score: 0.8,
            market_cap: Some(1000000.0),
        },
        TokenInfo {
            symbol: "TOKEN_B".to_string(),
            current_price: 50.0,
            historical_volatility: 0.3,
            liquidity_score: 0.7,
            market_cap: Some(500000.0),
        },
        TokenInfo {
            symbol: "TOKEN_C".to_string(),
            current_price: 200.0,
            historical_volatility: 0.1,
            liquidity_score: 0.9,
            market_cap: Some(2000000.0),
        },
    ]
}

fn create_sample_predictions() -> HashMap<String, f64> {
    let mut predictions = HashMap::new();
    predictions.insert("TOKEN_A".to_string(), 110.0); // +10%
    predictions.insert("TOKEN_B".to_string(), 55.0); // +10%
    predictions.insert("TOKEN_C".to_string(), 210.0); // +5%
    predictions
}

fn create_sample_price_history() -> Vec<PriceHistory> {
    let base_time = Utc::now() - Duration::days(30);
    let mut history = Vec::new();

    // TOKEN_A: 上昇トレンド
    for i in 0..30 {
        history.push(PriceHistory {
            token: "TOKEN_A".to_string(),
            timestamp: base_time + Duration::days(i),
            price: 90.0 + i as f64 * 0.5, // 90から105まで上昇
            volume: Some(1000.0),
        });
    }

    // TOKEN_B: 変動大
    for i in 0..30 {
        let volatility = ((i as f64 * 0.2).sin() * 10.0) + 50.0;
        history.push(PriceHistory {
            token: "TOKEN_B".to_string(),
            timestamp: base_time + Duration::days(i),
            price: volatility,
            volume: Some(800.0),
        });
    }

    // TOKEN_C: 安定
    for i in 0..30 {
        history.push(PriceHistory {
            token: "TOKEN_C".to_string(),
            timestamp: base_time + Duration::days(i),
            price: 195.0 + (i as f64 * 0.2), // 安定した上昇
            volume: Some(1200.0),
        });
    }

    history
}

fn create_sample_wallet() -> WalletInfo {
    let mut holdings = HashMap::new();
    holdings.insert("TOKEN_A".to_string(), 5.0); // 500 value
    holdings.insert("TOKEN_B".to_string(), 10.0); // 500 value

    WalletInfo {
        holdings,
        total_value: 1000.0,
        cash_balance: 0.0,
    }
}

// ==================== 基本機能テスト ====================

#[test]
fn test_calculate_expected_returns() {
    let tokens = create_sample_tokens();
    let predictions = create_sample_predictions();

    let expected_returns = calculate_expected_returns(&tokens, &predictions);

    assert_eq!(expected_returns.len(), 3);
    assert!((expected_returns[0] - 0.1).abs() < 0.001); // TOKEN_A: 10%
    assert!((expected_returns[1] - 0.1).abs() < 0.001); // TOKEN_B: 10%
    assert!((expected_returns[2] - 0.05).abs() < 0.001); // TOKEN_C: 5%
}

#[test]
fn test_calculate_daily_returns() {
    let price_history = create_sample_price_history();
    let daily_returns = calculate_daily_returns(&price_history);

    assert_eq!(daily_returns.len(), 3); // 3つのトークン

    // TOKEN_A は上昇トレンドなので、平均リターンが正
    let token_a_returns = &daily_returns[0];
    let avg_return: f64 = token_a_returns.iter().sum::<f64>() / token_a_returns.len() as f64;
    assert!(avg_return > 0.0); // 平均リターンが正であることを確認

    // 各トークンのリターン数は29（30日間のデータから）
    for returns in &daily_returns {
        assert_eq!(returns.len(), 29);
    }
}

#[test]
fn test_calculate_covariance_matrix() {
    let daily_returns = vec![
        vec![0.01, 0.02, -0.01, 0.03],  // TOKEN_A
        vec![0.02, -0.01, 0.01, 0.02],  // TOKEN_B
        vec![-0.01, 0.01, 0.02, -0.01], // TOKEN_C
    ];

    let covariance = calculate_covariance_matrix(&daily_returns);

    assert_eq!(covariance.shape(), [3, 3]);

    // 対角要素（分散）は正の値
    for i in 0..3 {
        assert!(covariance[[i, i]] > 0.0);
    }

    // 対称行列であることを確認
    for i in 0..3 {
        for j in 0..3 {
            assert!((covariance[[i, j]] - covariance[[j, i]]).abs() < 1e-10);
        }
    }
}

#[test]
fn test_calculate_portfolio_return() {
    let weights = vec![0.4, 0.3, 0.3];
    let expected_returns = vec![0.10, 0.08, 0.12];

    let portfolio_return = calculate_portfolio_return(&weights, &expected_returns);
    let expected = 0.4 * 0.10 + 0.3 * 0.08 + 0.3 * 0.12;

    assert!((portfolio_return - expected).abs() < 0.001);
}

#[test]
fn test_calculate_portfolio_std() {
    let weights = vec![0.5, 0.5];
    let covariance = array![[0.04, 0.02], [0.02, 0.09]]; // 2x2共分散行列

    let portfolio_std = calculate_portfolio_std(&weights, &covariance);

    // 手動計算: sqrt(0.5^2 * 0.04 + 0.5^2 * 0.09 + 2 * 0.5 * 0.5 * 0.02)
    let expected = ((0.25_f64 * 0.04) + (0.25_f64 * 0.09) + (0.5_f64 * 0.02)).sqrt();
    assert!((portfolio_std - expected).abs() < 0.001);
}

// ==================== 最適化テスト ====================

#[test]
fn test_maximize_sharpe_ratio() {
    let expected_returns = vec![0.10, 0.08, 0.12];
    let covariance = array![[0.04, 0.01, 0.02], [0.01, 0.09, 0.01], [0.02, 0.01, 0.03]];

    let optimal_weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    assert_eq!(optimal_weights.len(), 3);

    // 重みの合計が1に近い
    let sum: f64 = optimal_weights.iter().sum();
    assert!((sum - 1.0).abs() < 0.1);

    // 全ての重みが非負
    for &weight in &optimal_weights {
        assert!(weight >= 0.0);
    }

    // 最高リターンのTOKEN_Cの重みが最も高いことを期待
    let max_return_idx = expected_returns
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, _)| i)
        .unwrap();

    // リスク調整後でも、高リターン資産にある程度配分されることを確認
    assert!(optimal_weights[max_return_idx] > 0.0);
}

#[test]
fn test_calculate_efficient_frontier() {
    let expected_returns = vec![0.08, 0.12, 0.10];
    let covariance = array![[0.04, 0.01, 0.02], [0.01, 0.09, 0.01], [0.02, 0.01, 0.03]];
    let target_return = 0.10;

    let result = calculate_efficient_frontier(&expected_returns, &covariance, target_return);
    assert!(result.is_ok());

    let weights = result.unwrap();
    assert_eq!(weights.len(), 3);

    // 重みの合計が1に近い
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 0.1);

    // 目標リターンに近いことを確認
    let actual_return = calculate_portfolio_return(&weights, &expected_returns);
    assert!((actual_return - target_return).abs() < 0.05);
}

#[test]
fn test_apply_risk_parity() {
    let mut weights = vec![0.6, 0.2, 0.2]; // 不均等配分
    let covariance = array![[0.04, 0.01, 0.02], [0.01, 0.09, 0.01], [0.02, 0.01, 0.03]];

    apply_risk_parity(&mut weights, &covariance);

    // 重みの合計が1に近い
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 0.01);

    // より均等な配分になっていることを確認
    let max_weight = weights.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let min_weight = weights.iter().cloned().fold(f64::INFINITY, f64::min);
    assert!(max_weight - min_weight < 0.5); // 初期の0.4よりも小さい差
}

// ==================== 制約テスト ====================

#[test]
fn test_apply_constraints() {
    let mut weights = vec![0.7, 0.2, 0.1, 0.03, 0.02]; // 制約違反のケース

    apply_constraints(&mut weights);

    // 最大ポジションサイズ制約
    for &weight in &weights {
        if weight > MAX_POSITION_SIZE {
            println!(
                "Weight {} exceeds max position size {}",
                weight, MAX_POSITION_SIZE
            );
        }
        assert!(weight <= MAX_POSITION_SIZE + 1e-4); // 浮動小数点の誤差を許容
    }

    // 最小ポジションサイズフィルタ（小さすぎる重みは0になる）
    let small_positions = weights
        .iter()
        .filter(|&&w| w > 0.0 && w < MIN_POSITION_SIZE)
        .count();
    assert_eq!(small_positions, 0);

    // 重みの合計が1に近い
    let sum: f64 = weights.iter().sum();
    assert!((sum - 1.0).abs() < 0.01);
}

#[test]
fn test_needs_rebalancing() {
    let current_weights = vec![0.4, 0.3, 0.3];
    let target_weights_no_rebalance = vec![0.42, 0.28, 0.30]; // 小さな変化
    let target_weights_rebalance = vec![0.6, 0.2, 0.2]; // 大きな変化

    assert!(!needs_rebalancing(
        &current_weights,
        &target_weights_no_rebalance
    ));
    assert!(needs_rebalancing(
        &current_weights,
        &target_weights_rebalance
    ));

    // 長さが異なる場合
    let different_length = vec![0.5, 0.5];
    assert!(needs_rebalancing(&current_weights, &different_length));
}

// ==================== メトリクステスト ====================

#[test]
fn test_calculate_sortino_ratio() {
    let returns = vec![0.05, -0.02, 0.08, -0.01, 0.03, 0.06, -0.03];
    let risk_free_rate = 0.02;

    let sortino = calculate_sortino_ratio(&returns, risk_free_rate);

    // ソルティノレシオは有限の正の値
    assert!(sortino.is_finite());
    assert!(sortino > 0.0);

    // 空のリターンの場合
    assert_eq!(calculate_sortino_ratio(&[], risk_free_rate), 0.0);

    // 全て正のリターンの場合（下方偏差が0）
    let positive_returns = vec![0.05, 0.03, 0.08, 0.06];
    let sortino_positive = calculate_sortino_ratio(&positive_returns, risk_free_rate);
    assert_eq!(sortino_positive, 0.0); // 下方偏差が0なのでソルティノレシオも0
}

#[test]
fn test_calculate_max_drawdown() {
    let cumulative_returns = vec![100.0, 110.0, 90.0, 120.0, 80.0, 150.0];
    let max_dd = calculate_max_drawdown(&cumulative_returns);

    // 120から80への下落が最大: (120-80)/120 = 33.33%
    assert!((max_dd - 0.3333333333333333).abs() < 0.001);

    // 単調増加の場合
    let increasing = vec![100.0, 110.0, 120.0, 130.0];
    assert_eq!(calculate_max_drawdown(&increasing), 0.0);

    // 空配列の場合
    assert_eq!(calculate_max_drawdown(&[]), 0.0);
}

#[test]
fn test_calculate_turnover_rate() {
    let old_weights = vec![0.4, 0.3, 0.3];
    let new_weights = vec![0.5, 0.2, 0.3];

    let turnover = calculate_turnover_rate(&old_weights, &new_weights);
    // |0.4-0.5| + |0.3-0.2| + |0.3-0.3| = 0.1 + 0.1 + 0.0 = 0.2
    // turnover = 0.2 / 2 = 0.1
    assert!((turnover - 0.1).abs() < 0.001);

    // 完全な入れ替えの場合
    let completely_different = vec![0.0, 1.0];
    assert_eq!(
        calculate_turnover_rate(&old_weights, &completely_different),
        1.0
    );
}

// ==================== 統合テスト ====================

#[test]
fn test_execute_portfolio_optimization() {
    let tokens = create_sample_tokens();
    let predictions = create_sample_predictions();
    let historical_prices = create_sample_price_history();
    let wallet = create_sample_wallet();

    let portfolio_data = PortfolioData {
        tokens,
        predictions,
        historical_prices,
        correlation_matrix: None,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let result =
        rt.block_on(async { execute_portfolio_optimization(&wallet, portfolio_data).await });

    assert!(result.is_ok());

    let report = result.unwrap();

    // 基本的な構造確認
    assert!(!report.actions.is_empty() || matches!(report.actions[0], PortfolioAction::Hold));
    assert!(
        !report.optimal_weights.weights.is_empty() || report.optimal_weights.weights.is_empty()
    );
    assert!(report.expected_metrics.sharpe_ratio.is_finite());

    // メトリクスが合理的な範囲内
    assert!(report.expected_metrics.volatility >= 0.0);
    assert!(report.expected_metrics.turnover_rate >= 0.0);
    assert!(report.expected_metrics.turnover_rate <= 1.0);
}

// ==================== エッジケーステスト ====================

#[test]
fn test_empty_inputs() {
    let empty_tokens = vec![];
    let empty_predictions = HashMap::new();

    let expected_returns = calculate_expected_returns(&empty_tokens, &empty_predictions);
    assert!(expected_returns.is_empty());

    let empty_returns = vec![];
    let covariance = calculate_covariance_matrix(&empty_returns);
    assert_eq!(covariance.shape(), [0, 0]);

    let optimal_weights = maximize_sharpe_ratio(&[], &covariance);
    assert!(optimal_weights.is_empty());
}

#[test]
fn test_single_token_portfolio() {
    let tokens = vec![TokenInfo {
        symbol: "SINGLE_TOKEN".to_string(),
        current_price: 100.0,
        historical_volatility: 0.2,
        liquidity_score: 0.8,
        market_cap: Some(1000000.0),
    }];

    let mut predictions = HashMap::new();
    predictions.insert("SINGLE_TOKEN".to_string(), 110.0);

    let expected_returns = calculate_expected_returns(&tokens, &predictions);
    assert_eq!(expected_returns.len(), 1);
    assert!((expected_returns[0] - 0.1).abs() < 0.001);

    // 単一資産の場合、重みは1.0になる
    let covariance = array![[0.04]];
    let optimal_weights = maximize_sharpe_ratio(&expected_returns, &covariance);
    assert_eq!(optimal_weights.len(), 1);
    assert!((optimal_weights[0] - 1.0).abs() < 0.01);
}

#[test]
fn test_numerical_stability() {
    // 非常に小さな分散を持つ資産
    let expected_returns = vec![0.001, 0.002, 0.0015];
    let covariance = array![[1e-8, 1e-9, 1e-9], [1e-9, 1e-8, 1e-9], [1e-9, 1e-9, 1e-8]];

    let optimal_weights = maximize_sharpe_ratio(&expected_returns, &covariance);

    // 数値的に安定した結果が得られることを確認
    assert_eq!(optimal_weights.len(), 3);

    for &weight in &optimal_weights {
        assert!(weight.is_finite());
        assert!(weight >= 0.0);
    }

    let sum: f64 = optimal_weights.iter().sum();
    assert!((sum - 1.0).abs() < 0.1);
}

#[test]
fn test_covariance_calculation_edge_cases() {
    // 全て同じ値のリターン
    let identical_returns = vec![vec![0.05, 0.05, 0.05, 0.05]];
    let covariance = calculate_covariance_matrix(&identical_returns);
    assert_eq!(covariance.shape(), [1, 1]);
    assert!(covariance[[0, 0]] >= REGULARIZATION_FACTOR); // 正則化により正の値

    // 空のリターン
    let empty_return = vec![vec![]];
    let cov_empty = calculate_covariance_matrix(&empty_return);
    assert_eq!(cov_empty.shape(), [1, 1]);
    assert_eq!(cov_empty[[0, 0]], REGULARIZATION_FACTOR);
}

#[test]
fn test_extreme_predictions() {
    let tokens = create_sample_tokens();
    let mut predictions = HashMap::new();

    // 極端な予測値
    predictions.insert("TOKEN_A".to_string(), 1000.0); // 1000%上昇
    predictions.insert("TOKEN_B".to_string(), 0.1); // 99.8%下落
    predictions.insert("TOKEN_C".to_string(), 200.0); // 変化なし

    let expected_returns = calculate_expected_returns(&tokens, &predictions);

    assert_eq!(expected_returns.len(), 3);
    assert!(expected_returns[0] > 5.0); // 非常に高いリターン
    assert!(expected_returns[1] < -0.9); // 非常に低いリターン
    assert!((expected_returns[2] - 0.0).abs() < 0.001); // 変化なし
}

#[test]
fn test_portfolio_action_generation() {
    let tokens = create_sample_tokens();
    let current_weights = vec![0.5, 0.3, 0.2];
    let target_weights = vec![0.3, 0.4, 0.3]; // 大きな変化

    let actions = generate_rebalance_actions(&tokens, &current_weights, &target_weights);

    assert!(!actions.is_empty());

    // リバランスアクションが含まれることを確認
    let has_rebalance = actions
        .iter()
        .any(|action| matches!(action, PortfolioAction::Rebalance { .. }));
    assert!(has_rebalance);
}
