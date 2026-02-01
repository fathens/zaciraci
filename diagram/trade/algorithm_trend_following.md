# トレンドフォロー戦略 (Trend Following Strategy)

## 概要
24時間後の予測価格と過去の価格データから強いトレンドを識別し、トレンドが継続する方向にポジションを取る戦略。トレンド相場で最大のリターンを狙う。

## アルゴリズムの詳細

### 入力データ
```rust
struct TrendData {
    token: String,
    current_price: f64,
    predicted_price_24h: f64,
    price_history: Vec<PricePoint>,    // 過去7日間の価格データ
    volume_history: Vec<f64>,          // 取引量履歴
}

struct PricePoint {
    timestamp: DateTime<Utc>,
    price: f64,
    high: f64,
    low: f64,
}
```

### 主要パラメータ
```rust
const TREND_STRENGTH_THRESHOLD: f64 = 0.7;    // トレンド強度閾値
const MIN_TREND_DURATION: usize = 3;           // 最小トレンド継続期間（日）
const MOMENTUM_PERIOD: usize = 24;             // モメンタム計算期間（時間）
const BREAKOUT_MULTIPLIER: f64 = 1.2;          // ブレイクアウト判定倍率
const STOP_LOSS_PERCENTAGE: f64 = 0.05;        // ストップロス 5%
```

## 実装ステップ

### Step 1: トレンド強度の計算
```rust
fn calculate_trend_strength(price_history: &[PricePoint]) -> f64 {
    // 線形回帰でトレンドラインを計算
    let n = price_history.len() as f64;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    
    for (i, point) in price_history.iter().enumerate() {
        let x = i as f64;
        let y = point.price;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_xx += x * x;
    }
    
    // 傾きを計算
    let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_xx - sum_x * sum_x);
    
    // R²（決定係数）を計算してトレンド強度とする
    let mean_y = sum_y / n;
    let mut ss_tot = 0.0;
    let mut ss_res = 0.0;
    
    for (i, point) in price_history.iter().enumerate() {
        let predicted = slope * i as f64 + (mean_y - slope * sum_x / n);
        ss_tot += (point.price - mean_y).powi(2);
        ss_res += (point.price - predicted).powi(2);
    }
    
    let r_squared = 1.0 - (ss_res / ss_tot);
    r_squared * slope.signum() // 符号付きトレンド強度
}
```

### Step 2: モメンタムインジケーターの計算
```rust
fn calculate_momentum_indicators(trend_data: &TrendData) -> MomentumIndicators {
    let prices: Vec<f64> = trend_data.price_history.iter()
        .map(|p| p.price)
        .collect();
    
    MomentumIndicators {
        rsi: calculate_rsi(&prices, 14),
        macd: calculate_macd(&prices),
        adx: calculate_adx(&trend_data.price_history),
        volume_trend: calculate_volume_trend(&trend_data.volume_history),
    }
}

fn calculate_rsi(prices: &[f64], period: usize) -> f64 {
    let mut gains = 0.0;
    let mut losses = 0.0;
    
    for i in 1..period.min(prices.len()) {
        let change = prices[i] - prices[i - 1];
        if change > 0.0 {
            gains += change;
        } else {
            losses -= change;
        }
    }
    
    let avg_gain = gains / period as f64;
    let avg_loss = losses / period as f64;
    
    if avg_loss == 0.0 {
        100.0
    } else {
        100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
    }
}

fn calculate_adx(price_history: &[PricePoint]) -> f64 {
    // Average Directional Index の計算
    let mut plus_dm = Vec::new();
    let mut minus_dm = Vec::new();
    let mut tr = Vec::new();
    
    for i in 1..price_history.len() {
        let high_diff = price_history[i].high - price_history[i - 1].high;
        let low_diff = price_history[i - 1].low - price_history[i].low;
        
        plus_dm.push(if high_diff > low_diff && high_diff > 0.0 { high_diff } else { 0.0 });
        minus_dm.push(if low_diff > high_diff && low_diff > 0.0 { low_diff } else { 0.0 });
        
        // True Range
        let high_low = price_history[i].high - price_history[i].low;
        let high_close = (price_history[i].high - price_history[i - 1].price).abs();
        let low_close = (price_history[i].low - price_history[i - 1].price).abs();
        tr.push(high_low.max(high_close).max(low_close));
    }
    
    // ADX計算（簡略版）
    let avg_tr = tr.iter().sum::<f64>() / tr.len() as f64;
    let avg_plus_dm = plus_dm.iter().sum::<f64>() / plus_dm.len() as f64;
    let avg_minus_dm = minus_dm.iter().sum::<f64>() / minus_dm.len() as f64;
    
    let plus_di = 100.0 * avg_plus_dm / avg_tr;
    let minus_di = 100.0 * avg_minus_dm / avg_tr;
    let dx = 100.0 * ((plus_di - minus_di).abs() / (plus_di + minus_di));
    
    dx // 簡略版のため、DXをそのまま返す
}
```

### Step 3: ブレイクアウトの検出
```rust
fn detect_breakout(trend_data: &TrendData) -> BreakoutSignal {
    let recent_high = trend_data.price_history
        .iter()
        .map(|p| p.high)
        .max_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();
    
    let recent_low = trend_data.price_history
        .iter()
        .map(|p| p.low)
        .min_by(|a, b| a.partial_cmp(b).unwrap())
        .unwrap();
    
    let avg_volume = trend_data.volume_history.iter().sum::<f64>() 
        / trend_data.volume_history.len() as f64;
    let current_volume = trend_data.volume_history.last().unwrap_or(&0.0);
    
    // 上方ブレイクアウト
    if trend_data.current_price > recent_high 
        && current_volume > avg_volume * BREAKOUT_MULTIPLIER {
        return BreakoutSignal::Bullish;
    }
    
    // 下方ブレイクアウト
    if trend_data.current_price < recent_low 
        && current_volume > avg_volume * BREAKOUT_MULTIPLIER {
        return BreakoutSignal::Bearish;
    }
    
    BreakoutSignal::None
}
```

### Step 4: トレンド継続性の評価
```rust
fn evaluate_trend_continuation(
    trend_data: &TrendData,
    predicted_price: f64
) -> TrendContinuation {
    let trend_strength = calculate_trend_strength(&trend_data.price_history);
    let momentum = calculate_momentum_indicators(trend_data);
    
    // 予測価格がトレンドを支持するか
    let price_supports_trend = if trend_strength > 0.0 {
        predicted_price > trend_data.current_price
    } else {
        predicted_price < trend_data.current_price
    };
    
    // トレンド継続の総合スコア
    let continuation_score = calculate_continuation_score(
        trend_strength,
        momentum,
        price_supports_trend
    );
    
    TrendContinuation {
        score: continuation_score,
        confidence: trend_strength.abs(),
        expected_direction: if trend_strength > 0.0 { 
            Direction::Up 
        } else { 
            Direction::Down 
        },
    }
}

fn calculate_continuation_score(
    trend_strength: f64,
    momentum: MomentumIndicators,
    price_supports: bool
) -> f64 {
    let mut score = 0.0;
    
    // トレンド強度の寄与
    score += trend_strength.abs() * 0.3;
    
    // RSIの寄与（トレンドに沿った値なら加点）
    if trend_strength > 0.0 && momentum.rsi > 50.0 && momentum.rsi < 70.0 {
        score += 0.2;
    } else if trend_strength < 0.0 && momentum.rsi < 50.0 && momentum.rsi > 30.0 {
        score += 0.2;
    }
    
    // ADXの寄与（強いトレンドを示す）
    if momentum.adx > 25.0 {
        score += 0.2;
    }
    
    // 予測価格の寄与
    if price_supports {
        score += 0.3;
    }
    
    score
}
```

### Step 5: エントリー・エグジット戦略
```rust
struct TradingSignals {
    entry_conditions: Vec<EntryCondition>,
    exit_conditions: Vec<ExitCondition>,
    stop_loss: f64,
    take_profit: f64,
}

fn generate_trading_signals(
    trend_data: &TrendData,
    continuation: &TrendContinuation
) -> TradingSignals {
    let mut signals = TradingSignals::default();
    
    // エントリー条件
    if continuation.score > TREND_STRENGTH_THRESHOLD {
        // 強いトレンドでエントリー
        signals.entry_conditions.push(
            EntryCondition::StrongTrend {
                direction: continuation.expected_direction,
                strength: continuation.score,
            }
        );
    }
    
    // ブレイクアウトシグナル
    let breakout = detect_breakout(trend_data);
    if breakout != BreakoutSignal::None {
        signals.entry_conditions.push(
            EntryCondition::Breakout(breakout)
        );
    }
    
    // ストップロスとテイクプロフィット
    match continuation.expected_direction {
        Direction::Up => {
            signals.stop_loss = trend_data.current_price * (1.0 - STOP_LOSS_PERCENTAGE);
            signals.take_profit = trend_data.predicted_price_24h * 1.1;
        },
        Direction::Down => {
            signals.stop_loss = trend_data.current_price * (1.0 + STOP_LOSS_PERCENTAGE);
            signals.take_profit = trend_data.predicted_price_24h * 0.9;
        },
    }
    
    // エグジット条件
    signals.exit_conditions = vec![
        ExitCondition::TrendReversal,
        ExitCondition::TargetReached,
        ExitCondition::StopLossHit,
        ExitCondition::TimeBasedExit { hours: 24 },
    ];
    
    signals
}
```

### Step 6: 実行フロー
```rust
async fn execute_trend_following_strategy(
    wallet: &Wallet,
    market_data: Vec<TrendData>
) -> Result<ExecutionReport> {
    let mut positions = Vec::new();
    let mut signals_log = Vec::new();
    
    // 1. 各トークンのトレンド分析
    for trend_data in market_data {
        let continuation = evaluate_trend_continuation(
            &trend_data,
            trend_data.predicted_price_24h
        );
        
        let signals = generate_trading_signals(&trend_data, &continuation);
        
        // 2. エントリー判定
        if should_enter_position(&signals) {
            let position = Position {
                token: trend_data.token.clone(),
                direction: continuation.expected_direction,
                size: calculate_position_size(&continuation, wallet).await?,
                entry_price: trend_data.current_price,
                stop_loss: signals.stop_loss,
                take_profit: signals.take_profit,
            };
            
            positions.push(position.clone());
            
            // 3. ポジション実行
            execute_position(wallet, &position).await?;
            
            signals_log.push(SignalLog {
                token: trend_data.token,
                action: "ENTER",
                reason: format!("Trend score: {:.2}", continuation.score),
            });
        }
    }
    
    // 4. 既存ポジションの管理
    let existing_positions = wallet.get_positions().await?;
    for position in existing_positions {
        if should_exit_position(&position).await? {
            close_position(wallet, &position).await?;
            
            signals_log.push(SignalLog {
                token: position.token,
                action: "EXIT",
                reason: "Exit condition met",
            });
        }
    }
    
    Ok(ExecutionReport {
        positions,
        signals: signals_log,
        timestamp: Utc::now(),
    })
}

fn should_enter_position(signals: &TradingSignals) -> bool {
    // 複数のエントリー条件を確認
    signals.entry_conditions.len() >= 2 
        || signals.entry_conditions.iter().any(|c| {
            matches!(c, EntryCondition::StrongTrend { strength, .. } if strength > &0.8)
        })
}

async fn calculate_position_size(
    continuation: &TrendContinuation,
    wallet: &Wallet
) -> Result<f64> {
    let available_balance = wallet.get_available_balance().await?;
    
    // Kelly Criterion の簡略版
    let win_probability = continuation.score;
    let win_loss_ratio = 2.0; // 仮定：勝利時は2倍のリターン
    
    let kelly_fraction = (win_probability * win_loss_ratio - (1.0 - win_probability)) 
        / win_loss_ratio;
    
    // 安全係数を適用（Kelly の25%）
    let position_size = available_balance * kelly_fraction.max(0.0).min(0.25);
    
    Ok(position_size)
}
```

## 高度な機能

### マルチタイムフレーム分析
```rust
fn multi_timeframe_analysis(
    hourly_data: &[PricePoint],
    daily_data: &[PricePoint],
    weekly_data: &[PricePoint]
) -> TimeframeAlignment {
    let hourly_trend = calculate_trend_strength(hourly_data);
    let daily_trend = calculate_trend_strength(daily_data);
    let weekly_trend = calculate_trend_strength(weekly_data);
    
    // 全タイムフレームが同じ方向を示す場合、強いシグナル
    if hourly_trend > 0.0 && daily_trend > 0.0 && weekly_trend > 0.0 {
        TimeframeAlignment::StrongBullish
    } else if hourly_trend < 0.0 && daily_trend < 0.0 && weekly_trend < 0.0 {
        TimeframeAlignment::StrongBearish
    } else {
        TimeframeAlignment::Mixed
    }
}
```

### ボラティリティ調整
```rust
fn adjust_for_volatility(
    position_size: f64,
    historical_volatility: f64,
    target_volatility: f64
) -> f64 {
    // ボラティリティに反比例してポジションサイズを調整
    position_size * (target_volatility / historical_volatility).min(2.0).max(0.5)
}
```

## 利点と欠点

### 利点
- トレンド相場で最大のリターンを獲得可能
- 明確なエントリー・エグジットルール
- 大きな市場の動きを捕捉

### 欠点
- レンジ相場では損失が蓄積
- 偽のブレイクアウトによる損失リスク
- 遅行性があるため、トレンド転換の初動を逃す

## バックテスト指標
```rust
struct TrendFollowingMetrics {
    total_trades: usize,
    winning_trades: usize,
    average_win: f64,
    average_loss: f64,
    profit_factor: f64,          // 総利益 / 総損失
    max_consecutive_losses: usize,
    average_holding_period: Duration,
    trend_capture_ratio: f64,    // 捕捉したトレンドの割合
}
```

## リスク管理

### ポジションサイジング
```rust
const MAX_RISK_PER_TRADE: f64 = 0.02; // 1トレードあたり最大2%のリスク

fn calculate_safe_position_size(
    account_balance: f64,
    stop_loss_distance: f64
) -> f64 {
    (account_balance * MAX_RISK_PER_TRADE) / stop_loss_distance
}
```

### トレーリングストップ
```rust
fn update_trailing_stop(
    position: &mut Position,
    current_price: f64
) {
    let trailing_distance = position.entry_price * 0.03; // 3%のトレーリング
    
    if position.direction == Direction::Up {
        let new_stop = current_price - trailing_distance;
        position.stop_loss = position.stop_loss.max(new_stop);
    } else {
        let new_stop = current_price + trailing_distance;
        position.stop_loss = position.stop_loss.min(new_stop);
    }
}
```