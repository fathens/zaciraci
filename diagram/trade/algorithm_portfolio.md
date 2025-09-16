# ポートフォリオ最適化戦略 (Portfolio Optimization Strategy)

## 概要
現代ポートフォリオ理論に基づき、リスクとリターンのバランスを最適化する分散投資戦略。24時間後の予測値と過去データから最適な資産配分を決定。

## アルゴリズムの詳細

### 入力データ
```rust
struct PortfolioData {
    tokens: Vec<TokenInfo>,
    predictions: HashMap<String, f64>,      // 24時間後予測価格
    historical_prices: Vec<PriceHistory>,   // 過去30日間の価格データ
    correlation_matrix: Array2<f64>,        // トークン間相関行列
}

struct TokenInfo {
    symbol: String,
    current_price: f64,
    historical_volatility: f64,
    liquidity_score: f64,
}
```

### 主要パラメータ
```rust
const RISK_FREE_RATE: f64 = 0.02;       // 年率2%のリスクフリーレート
const MAX_POSITION_SIZE: f64 = 0.4;      // 単一トークンの最大保有比率
const MIN_POSITION_SIZE: f64 = 0.05;     // 最小保有比率
const REBALANCE_THRESHOLD: f64 = 0.1;    // リバランス閾値 10%
```

## 実装ステップ

### Step 1: 期待リターンと分散の計算
```rust
fn calculate_expected_returns(
    tokens: &[TokenInfo],
    predictions: &HashMap<String, f64>
) -> Vec<f64> {
    tokens.iter().map(|token| {
        let predicted = predictions.get(&token.symbol).unwrap();
        (predicted - token.current_price) / token.current_price
    }).collect()
}

fn calculate_covariance_matrix(
    historical_prices: &[PriceHistory]
) -> Array2<f64> {
    // 過去30日間の日次リターンから共分散行列を計算
    let returns = calculate_daily_returns(historical_prices);
    let n = returns.len();
    let mut covariance = Array2::zeros((n, n));
    
    for i in 0..n {
        for j in 0..n {
            covariance[[i, j]] = calculate_covariance(&returns[i], &returns[j]);
        }
    }
    
    covariance
}
```

### Step 2: 効率的フロンティアの計算
```rust
fn calculate_efficient_frontier(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    target_return: f64
) -> Result<Vec<f64>> {
    // 二次計画問題を解く
    // minimize: w^T * Σ * w (ポートフォリオの分散)
    // subject to: 
    //   - w^T * μ = target_return (目標リターン制約)
    //   - Σw_i = 1 (重みの合計が1)
    //   - 0 <= w_i <= MAX_POSITION_SIZE (個別制約)
    
    let n = expected_returns.len();
    let mut weights = vec![1.0 / n as f64; n]; // 初期値: 等配分
    
    // 最適化アルゴリズム (簡略版)
    for _ in 0..100 {
        weights = optimize_weights(
            &weights,
            expected_returns,
            covariance_matrix,
            target_return
        );
    }
    
    Ok(weights)
}
```

### Step 3: シャープレシオ最大化
```rust
fn maximize_sharpe_ratio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>
) -> Vec<f64> {
    // シャープレシオ = (E[R] - Rf) / σ
    let mut best_weights = vec![];
    let mut best_sharpe = f64::NEG_INFINITY;
    
    // グリッドサーチまたは勾配法で最適化
    for target_return in generate_target_returns() {
        let weights = calculate_efficient_frontier(
            expected_returns,
            covariance_matrix,
            target_return
        ).unwrap_or_default();
        
        let portfolio_return = calculate_portfolio_return(&weights, expected_returns);
        let portfolio_std = calculate_portfolio_std(&weights, covariance_matrix);
        
        let sharpe = (portfolio_return - RISK_FREE_RATE) / portfolio_std;
        
        if sharpe > best_sharpe {
            best_sharpe = sharpe;
            best_weights = weights;
        }
    }
    
    best_weights
}
```

### Step 4: リスクパリティ調整
```rust
fn apply_risk_parity(
    weights: &mut [f64],
    covariance_matrix: &Array2<f64>
) {
    // 各資産のリスク寄与度を均等化
    let total_risk = calculate_portfolio_std(weights, covariance_matrix);
    
    for i in 0..weights.len() {
        let marginal_risk = calculate_marginal_risk(i, weights, covariance_matrix);
        let risk_contribution = weights[i] * marginal_risk;
        
        // 目標リスク寄与度
        let target_contribution = total_risk / weights.len() as f64;
        
        // 重みを調整
        weights[i] *= target_contribution / risk_contribution;
    }
    
    // 正規化
    let sum: f64 = weights.iter().sum();
    for w in weights.iter_mut() {
        *w /= sum;
    }
}
```

### Step 5: 実行フロー
```rust
async fn execute_portfolio_optimization(
    wallet: &Wallet,
    portfolio_data: PortfolioData
) -> Result<ExecutionReport> {
    // 1. 期待リターンと共分散行列を計算
    let expected_returns = calculate_expected_returns(
        &portfolio_data.tokens,
        &portfolio_data.predictions
    );
    let covariance = calculate_covariance_matrix(
        &portfolio_data.historical_prices
    );
    
    // 2. 最適ポートフォリオを計算
    let mut optimal_weights = maximize_sharpe_ratio(
        &expected_returns,
        &covariance
    );
    
    // 3. リスクパリティ調整（オプション）
    apply_risk_parity(&mut optimal_weights, &covariance);
    
    // 4. 制約を適用
    apply_constraints(&mut optimal_weights);
    
    // 5. 現在のポートフォリオと比較
    let current_weights = wallet.get_portfolio_weights().await?;
    let rebalance_needed = needs_rebalancing(&current_weights, &optimal_weights);
    
    // 6. リバランス実行
    if rebalance_needed {
        execute_rebalance(wallet, &current_weights, &optimal_weights).await?;
    }
    
    Ok(ExecutionReport::new(optimal_weights))
}
```

### Step 6: 制約の適用
```rust
fn apply_constraints(weights: &mut [f64]) {
    // 最大・最小ポジションサイズ制約
    for w in weights.iter_mut() {
        *w = w.max(MIN_POSITION_SIZE).min(MAX_POSITION_SIZE);
    }
    
    // 上位N個のトークンのみ保有
    const MAX_HOLDINGS: usize = 10;
    let mut indexed_weights: Vec<_> = weights.iter().enumerate().collect();
    indexed_weights.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());
    
    for i in MAX_HOLDINGS..indexed_weights.len() {
        weights[indexed_weights[i].0] = 0.0;
    }
    
    // 再正規化
    let sum: f64 = weights.iter().sum();
    for w in weights.iter_mut() {
        *w /= sum;
    }
}
```

## 高度な最適化手法

### Black-Litterman モデル
```rust
fn black_litterman_optimization(
    market_cap_weights: &[f64],
    views: &ViewMatrix,
    confidence: &[f64]
) -> Vec<f64> {
    // 市場均衡リターンを計算
    let equilibrium_returns = calculate_equilibrium_returns(market_cap_weights);
    
    // 投資家の見解を組み込む
    let posterior_returns = combine_views_with_equilibrium(
        equilibrium_returns,
        views,
        confidence
    );
    
    // 最適化
    maximize_utility(posterior_returns)
}
```

### CVaR (Conditional Value at Risk) 最適化
```rust
fn optimize_cvar(
    scenarios: &[ScenarioReturns],
    confidence_level: f64
) -> Vec<f64> {
    // 最悪シナリオでの損失を最小化
    let var_threshold = calculate_var(scenarios, confidence_level);
    
    minimize_expected_loss_beyond_var(scenarios, var_threshold)
}
```

## 利点と欠点

### 利点
- 理論的に最もリスク効率的なポートフォリオ
- 分散投資によるリスク低減
- 市場変動への耐性

### 欠点
- 実装が複雑で計算コストが高い
- 過去データへの依存
- パラメータ推定誤差の影響

## バックテスト指標
```rust
struct PortfolioMetrics {
    cumulative_return: f64,
    annualized_return: f64,
    volatility: f64,
    sharpe_ratio: f64,
    sortino_ratio: f64,
    max_drawdown: f64,
    calmar_ratio: f64,
    turnover_rate: f64,
}
```

## 実装上の注意点

1. **数値安定性**: 共分散行列の逆行列計算時の正則化
2. **取引コスト**: リバランス頻度の最適化
3. **流動性制約**: 大口取引のマーケットインパクト考慮
4. **計算効率**: 大規模ポートフォリオでの最適化アルゴリズム選択