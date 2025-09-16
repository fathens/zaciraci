# モメンタムベース戦略 (Momentum-Based Strategy)

## 概要
24時間後の予測価格上昇率が最も高いトークンに資産を集中させる戦略。シンプルな実装で高リターンを狙う。

## アルゴリズムの詳細

### 入力データ
```rust
struct PredictionData {
    token: String,
    current_price: f64,
    predicted_price_24h: f64,
    timestamp: DateTime<Utc>,
}
```

### 主要パラメータ
```rust
const MIN_PROFIT_THRESHOLD: f64 = 0.05;  // 最低利益率 5%
const SWITCH_MULTIPLIER: f64 = 1.5;      // 切り替え倍率
const TOP_N_TOKENS: usize = 3;           // 上位N個のトークンを考慮
```

## 実装ステップ

### Step 1: 予測リターンの計算
```rust
fn calculate_expected_return(prediction: &PredictionData) -> f64 {
    (prediction.predicted_price_24h - prediction.current_price) 
        / prediction.current_price
}
```

### Step 2: トークンのランキング
```rust
fn rank_tokens_by_momentum(
    predictions: Vec<PredictionData>
) -> Vec<(String, f64)> {
    let mut ranked: Vec<_> = predictions
        .iter()
        .map(|p| (p.token.clone(), calculate_expected_return(p)))
        .collect();
    
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    ranked
}
```

### Step 3: 取引判断ロジック
```rust
fn make_trading_decision(
    current_token: &str,
    current_return: f64,
    ranked_tokens: &[(String, f64)]
) -> TradingAction {
    let best_token = &ranked_tokens[0];
    
    // 現在のトークンの期待リターンが閾値以下
    if current_return < MIN_PROFIT_THRESHOLD {
        return TradingAction::Sell {
            token: current_token.to_string(),
            target: best_token.0.clone(),
        };
    }
    
    // より良いトークンが存在する場合
    if best_token.1 > current_return * SWITCH_MULTIPLIER {
        return TradingAction::Switch {
            from: current_token.to_string(),
            to: best_token.0.clone(),
        };
    }
    
    TradingAction::Hold
}
```

### Step 4: 実行フロー
```rust
async fn execute_momentum_strategy(
    wallet: &Wallet,
    predictions: Vec<PredictionData>
) -> Result<ExecutionReport> {
    // 1. 現在の保有トークンを取得
    let current_holdings = wallet.get_holdings().await?;
    
    // 2. トークンをランキング
    let ranked = rank_tokens_by_momentum(predictions);
    
    // 3. 各保有トークンについて判断
    let mut actions = Vec::new();
    for holding in current_holdings {
        let current_return = calculate_expected_return(&holding);
        let action = make_trading_decision(
            &holding.token,
            current_return,
            &ranked
        );
        actions.push(action);
    }
    
    // 4. 取引を実行
    for action in actions {
        match action {
            TradingAction::Sell { token, target } => {
                wallet.swap_all(token, target).await?;
            },
            TradingAction::Switch { from, to } => {
                wallet.swap_all(from, to).await?;
            },
            TradingAction::Hold => {},
        }
    }
    
    Ok(ExecutionReport::new(actions))
}
```

## 利点と欠点

### 利点
- 実装が簡単で理解しやすい
- トレンド相場で高いリターンが期待できる
- 計算コストが低い

### 欠点
- 単一トークンへの集中リスク
- 予測精度に完全依存
- 取引コストを考慮していない

## バックテスト指標
```rust
struct BacktestMetrics {
    total_return: f64,
    max_drawdown: f64,
    sharpe_ratio: f64,
    win_rate: f64,
    avg_holding_period: Duration,
}
```

## 改善案

### 1. 取引コストの考慮
```rust
fn adjust_for_trading_costs(expected_return: f64) -> f64 {
    const TRADING_FEE: f64 = 0.003; // 0.3%
    expected_return - (2.0 * TRADING_FEE) // 往復の手数料
}
```

### 2. 部分的なポジション変更
```rust
fn calculate_position_size(
    confidence_score: f64,
    expected_return: f64
) -> f64 {
    // 信頼度と期待リターンに基づくポジションサイズ
    (confidence_score * expected_return).min(1.0).max(0.0)
}
```

### 3. ボラティリティフィルター
```rust
fn filter_by_volatility(
    tokens: Vec<(String, f64)>,
    max_volatility: f64
) -> Vec<(String, f64)> {
    tokens.into_iter()
        .filter(|(token, _)| {
            calculate_volatility(token) < max_volatility
        })
        .collect()
}
```