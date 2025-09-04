# CLI Tokens - Simulate Command

## 概要

`simulate`コマンドは、実際の価格データを使用してトレーディングアルゴリズムのバックテストとパフォーマンス分析を行う機能です。期間を指定して、その期間内で予測・取引を実際に実行したシミュレーションを行い、資産の変動を確認できます。

## 主な機能

- **バックテスト実行**: 過去のデータを使用して戦略の有効性を検証
- **実取引コスト計算**: Ref Financeの実際の手数料体系を反映
- **複数アルゴリズム対応**: momentum、portfolio、trend_followingの3つの戦略
- **自動データ取得**: 指定期間の価格データを自動取得
- **パフォーマンス分析**: リターン、シャープレシオ、最大ドローダウンなどの指標を計算

## コマンド仕様

### 基本構文
```bash
cli_tokens simulate [OPTIONS]
```

### オプション
```bash
OPTIONS:
    -s, --start <DATE>           シミュレーション開始日 (YYYY-MM-DD)
    -e, --end <DATE>             シミュレーション終了日 (YYYY-MM-DD)
    -a, --algorithm <ALGORITHM>  使用するアルゴリズム [デフォルト: momentum]
                                選択肢: momentum, portfolio, trend_following
    -c, --capital <AMOUNT>       初期資金 (NEAR) [デフォルト: 1000.0]
    -q, --quote-token <TOKEN>    ベース通貨 [デフォルト: wrap.near]
    -t, --tokens <TOKENS>        対象トークンリスト (カンマ区切り)
                                省略時は自動でtop volatility tokensを取得
    -n, --num-tokens <NUMBER>    自動取得する際のトークン数 [デフォルト: 10]
    -o, --output <DIR>           出力ディレクトリ [デフォルト: simulation_results/]
    --rebalance-freq <FREQ>      リバランス頻度 [デフォルト: daily]
                                選択肢: hourly, daily, weekly
    --fee-model <MODEL>          手数料モデル [デフォルト: realistic]
                                選択肢: realistic, zero, custom
    --custom-fee <RATE>          カスタム手数料率 (0.0-1.0)
    --slippage <RATE>            スリッページ率 (0.0-1.0) [デフォルト: 0.01]
    --gas-cost <AMOUNT>          ガス料金 (NEAR) [デフォルト: 0.01]
    --min-trade <AMOUNT>         最小取引額 (NEAR) [デフォルト: 1.0]
    --prediction-horizon <HOURS> 予測期間 (時間) [デフォルト: 24]
    --historical-days <DAYS>     予測に使用する過去データ期間 (日数) [デフォルト: 30]
    --chart                      チャートを生成 (未実装)
    --verbose                    詳細ログ
    -h, --help                   ヘルプを表示
```

### 使用例

#### 基本的なシミュレーション
```bash
export CLI_TOKENS_BASE_DIR="./workspace"

# 1ヶ月間のモメンタム戦略シミュレーション
cli_tokens simulate \
  --start 2024-12-01 \
  --end 2024-12-31 \
  --algorithm momentum \
  --capital 1000 \
  --output simulation_results

# 指定トークンでのポートフォリオ最適化
cli_tokens simulate \
  --start 2024-11-01 \
  --end 2024-12-01 \
  --algorithm portfolio \
  --tokens "usdc.tether-token.near,blackdragon.tkn.near,meow.token.near" \
  --capital 5000 \
  --rebalance-freq weekly
```

#### 高度な設定
```bash
# カスタム手数料でのシミュレーション
cli_tokens simulate \
  --start 2024-10-01 \
  --end 2024-11-01 \
  --algorithm trend_following \
  --fee-model custom \
  --custom-fee 0.005 \
  --slippage 0.02 \
  --gas-cost 0.02

# レポート生成（別コマンド）
cli_tokens simulate \
  --start 2024-09-01 \
  --end 2024-12-01 \
  --algorithm momentum \
  --verbose

# 結果からHTMLレポート生成  
cli_tokens report simulation_results/momentum_2024-09-01_2024-12-01/results.json \
  --format html
```

## シミュレーションの動作

### タイムステップ処理

シミュレーションは指定された期間内で、リバランス頻度に従って時系列で実行されます：

1. **価格データ取得**: 指定期間とヒストリカルデータ期間の価格を取得
2. **初期ポートフォリオ構築**: 初期資金を指定トークンに配分
3. **各タイムステップで実行**:
   - 現在価格と過去データから予測を生成
   - アルゴリズムによる取引判断
   - 取引実行とコスト計算
   - ポートフォリオ価値更新
4. **パフォーマンス分析**: 全取引完了後に各種指標を計算

### リバランス頻度

- **hourly**: 1時間ごとに取引判断（高頻度取引）
- **daily**: 1日ごとに取引判断（デフォルト）
- **weekly**: 週1回取引判断（低頻度取引）

### 必要データ期間

シミュレーションには以下の期間のデータが必要です：

- **開始**: `start_date - historical_days`
- **終了**: `end_date + prediction_horizon`

例：2024-11-01～2024-11-30のシミュレーション（historical_days=30、prediction_horizon=24時間）
→ 必要データ: 2024-10-02～2024-12-01

## 実装詳細

### 1. 取引コスト計算

#### 基本構造体
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingCost {
    pub protocol_fee: BigDecimal,    // DEX手数料
    pub slippage: BigDecimal,        // スリッページ
    pub gas_fee: BigDecimal,         // ガス料金
    pub total: BigDecimal,           // 総コスト
}

#[derive(Debug, Clone)]
pub enum FeeModel {
    Realistic,                       // 実際のRef Financeの手数料
    Zero,                           // 手数料なし（理想的シミュレーション）
    Custom(f64),                    // カスタム手数料率
}
```

#### 取引コスト計算関数
```rust
pub fn calculate_trading_cost(
    amount: &BigDecimal,
    fee_model: &FeeModel,
    pool_fee_rate: Option<f64>,      // プール固有の手数料（total_fee/10000）
    slippage_rate: f64,
    gas_cost: BigDecimal,
) -> TradingCost {
    let protocol_fee = match fee_model {
        FeeModel::Realistic => {
            let rate = pool_fee_rate.unwrap_or(0.003); // 0.3% デフォルト
            amount * BigDecimal::from_f64(rate).unwrap()
        },
        FeeModel::Zero => BigDecimal::zero(),
        FeeModel::Custom(rate) => amount * BigDecimal::from_f64(*rate).unwrap(),
    };
    
    let slippage = amount * BigDecimal::from_f64(slippage_rate).unwrap();
    let total = &protocol_fee + &slippage + &gas_cost;
    
    TradingCost {
        protocol_fee,
        slippage,
        gas_fee: gas_cost,
        total,
    }
}
```

### 2. シミュレーションエンジン

#### 主要構造体
```rust
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub algorithm: AlgorithmType,
    pub initial_capital: BigDecimal,
    pub quote_token: String,
    pub target_tokens: Vec<String>,
    pub rebalance_frequency: RebalanceFrequency,
    pub fee_model: FeeModel,
    pub slippage_rate: f64,
    pub gas_cost: BigDecimal,
    pub min_trade_amount: BigDecimal,
    pub prediction_horizon: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub config: SimulationConfig,
    pub trades: Vec<TradeExecution>,
    pub portfolio_values: Vec<PortfolioValue>,
    pub performance: PerformanceMetrics,
    pub algorithm_specific: serde_json::Value,
    pub execution_summary: ExecutionSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeExecution {
    pub timestamp: DateTime<Utc>,
    pub action: TradingAction,
    pub from_token: String,
    pub to_token: String,
    pub amount: BigDecimal,
    pub executed_price: BigDecimal,
    pub cost: TradingCost,
    pub portfolio_value_before: BigDecimal,
    pub portfolio_value_after: BigDecimal,
    pub success: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioValue {
    pub timestamp: DateTime<Utc>,
    pub total_value: BigDecimal,
    pub holdings: HashMap<String, BigDecimal>,
    pub cash_balance: BigDecimal,
    pub unrealized_pnl: BigDecimal,
}
```

#### シミュレーション実行フロー
```rust
pub async fn run_simulation(config: SimulationConfig) -> Result<SimulationResult> {
    // 1. 価格データ取得
    let price_data = fetch_historical_data(&config).await?;
    
    // 2. アルゴリズム初期化
    let algorithm = initialize_algorithm(&config.algorithm).await?;
    
    // 3. 初期ポートフォリオ設定
    let mut portfolio = Portfolio::new(config.initial_capital, config.quote_token.clone());
    
    // 4. 時系列シミュレーション
    let mut trades = Vec::new();
    let mut portfolio_values = Vec::new();
    
    let time_step = match config.rebalance_frequency {
        RebalanceFrequency::Hourly => Duration::hours(1),
        RebalanceFrequency::Daily => Duration::days(1),
        RebalanceFrequency::Weekly => Duration::days(7),
    };
    
    let mut current_time = config.start_date;
    while current_time <= config.end_date {
        // 4.1 現在時点での予測実行
        let predictions = run_predictions(&algorithm, &price_data, current_time, &config).await?;
        
        // 4.2 取引判断
        let trading_decision = algorithm.generate_trading_signals(&predictions, &portfolio)?;
        
        // 4.3 取引実行
        if let Some(actions) = trading_decision {
            let executed_trades = execute_trades(
                actions,
                &mut portfolio,
                &price_data,
                current_time,
                &config,
            ).await?;
            trades.extend(executed_trades);
        }
        
        // 4.4 ポートフォリオ価値記録
        let current_value = calculate_portfolio_value(&portfolio, &price_data, current_time)?;
        portfolio_values.push(current_value);
        
        current_time += time_step;
    }
    
    // 5. パフォーマンス分析
    let performance = calculate_performance_metrics(&trades, &portfolio_values)?;
    
    Ok(SimulationResult {
        config,
        trades,
        portfolio_values,
        performance,
        algorithm_specific: serde_json::Value::Null,
        execution_summary: ExecutionSummary::from(&trades),
    })
}
```

### 3. パフォーマンス分析

#### パフォーマンス指標
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    // 基本指標
    pub total_return: f64,                    // 総収益率
    pub annualized_return: f64,               // 年率換算収益率
    pub total_return_pct: f64,                // 総収益率（%）
    
    // リスク指標
    pub volatility: f64,                      // ボラティリティ
    pub max_drawdown: f64,                    // 最大ドローダウン
    pub max_drawdown_pct: f64,                // 最大ドローダウン（%）
    pub sharpe_ratio: f64,                    // シャープレシオ
    pub sortino_ratio: f64,                   // ソルティノレシオ
    pub calmar_ratio: f64,                    // カルマーレシオ
    
    // 取引指標
    pub total_trades: usize,                  // 総取引回数
    pub winning_trades: usize,                // 勝率取引数
    pub losing_trades: usize,                 // 負率取引数
    pub win_rate: f64,                        // 勝率
    pub profit_factor: f64,                   // プロフィットファクター
    pub avg_win_pct: f64,                     // 平均勝ち取引（%）
    pub avg_loss_pct: f64,                    // 平均負け取引（%）
    
    // コスト指標
    pub total_costs: BigDecimal,              // 総取引コスト
    pub cost_ratio: f64,                      // コスト比率（総収益に対する%）
    pub avg_cost_per_trade: BigDecimal,       // 1取引あたり平均コスト
    
    // 期間指標
    pub simulation_days: i64,                 // シミュレーション日数
    pub active_trading_days: i64,             // 実際に取引があった日数
    pub avg_holding_period: Duration,         // 平均保有期間
}
```

#### ベンチマーク比較
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub strategy_performance: PerformanceMetrics,
    pub buy_and_hold: PerformanceMetrics,
    pub market_index: Option<PerformanceMetrics>,
    pub outperformance: f64,
    pub alpha: f64,
    pub beta: f64,
    pub information_ratio: f64,
}
```

### 4. レポート生成

#### JSON形式
```json
{
  "simulation_summary": {
    "start_date": "2024-12-01T00:00:00Z",
    "end_date": "2024-12-31T23:59:59Z",
    "algorithm": "momentum",
    "initial_capital": 1000.0,
    "final_value": 1125.45,
    "total_return": 12.55,
    "duration_days": 31
  },
  "performance_metrics": {
    "total_return": 0.12545,
    "annualized_return": 1.51,
    "sharpe_ratio": 1.23,
    "max_drawdown": -0.087,
    "win_rate": 0.68,
    "total_trades": 23
  },
  "trades": [],
  "portfolio_evolution": [],
  "benchmark_comparison": {}
}
```

#### HTML形式
- インタラクティブチャート（Chart.js使用）
- 詳細な取引履歴テーブル
- パフォーマンス指標の視覚化
- アルゴリズム固有の分析結果

### 5. アルゴリズム統合

#### 既存アルゴリズムの活用
```rust
// backend/src/trade/algorithm/momentum.rs の機能を活用
use zaciraci_backend::trade::algorithm::momentum::{
    execute_momentum_strategy,
    calculate_expected_return,
    rank_tokens_by_momentum,
    make_trading_decision,
};

// backend/src/trade/algorithm/portfolio.rs の機能を活用
use zaciraci_backend::trade::algorithm::portfolio::{
    execute_portfolio_optimization,
    maximize_sharpe_ratio,
    calculate_expected_returns,
    needs_rebalancing,
};
```

#### 予測機能の統合
```rust
// backend/src/trade/predict.rs の PredictionService を活用
pub async fn run_predictions(
    algorithm: &AlgorithmType,
    price_data: &PriceData,
    current_time: DateTime<Utc>,
    config: &SimulationConfig,
) -> Result<HashMap<String, TokenPrediction>> {
    let prediction_service = PredictionService::new(
        "http://localhost:8000".to_string(), // Chronos URL
        "http://localhost:8080".to_string(), // Backend URL
    );
    
    let end_time = current_time;
    let start_time = current_time - Duration::days(30); // 30日履歴
    
    let mut predictions = HashMap::new();
    for token in &config.target_tokens {
        let history = prediction_service
            .get_price_history(token, &config.quote_token, start_time, end_time)
            .await?;
            
        let prediction = prediction_service
            .predict_price(&history, config.prediction_horizon.num_hours() as usize)
            .await?;
            
        predictions.insert(token.clone(), prediction);
    }
    
    Ok(predictions)
}
```

## 出力ファイル構造

```
${CLI_TOKENS_BASE_DIR}/
└── simulation_results/
    ├── momentum_2024-12-01_2024-12-31/
    │   ├── config.json                    # シミュレーション設定
    │   ├── results.json                   # メイン結果（JSON形式）
    │   ├── results.html                   # HTMLレポート（reportコマンドで生成）
    │   ├── trades.csv                     # 取引履歴（CSV形式）
    │   ├── portfolio_values.csv           # ポートフォリオ価値推移
    │   ├── performance_chart.png          # パフォーマンスチャート（--chart）
    │   └── logs/                          # 詳細ログ（--verbose）
    │       ├── execution.log
    │       └── predictions.log
    └── portfolio_2024-11-01_2024-12-01/
        └── ... (同様の構造)
```


## reportコマンド

シミュレーション結果からHTMLレポートを生成する独立したコマンドです。

### 基本構文
```bash
cli_tokens report <INPUT_JSON> [OPTIONS]
```

### オプション
```bash
OPTIONS:
    -f, --format <FORMAT>    出力形式 [デフォルト: html]
                            現在は html のみサポート
    -o, --output <PATH>     出力ファイルパス（オプション）
    -h, --help              ヘルプを表示
```

### 使用例
```bash
# HTMLレポート生成
cli_tokens report simulation_results/momentum_2024-12-01_2024-12-31/results.json

# 出力先を指定
cli_tokens report results.json --output custom_report.html
```

## 注意事項

- **データ可用性**: シミュレーション期間の価格データが存在することを確認してください
- **バックエンドAPI**: `http://localhost:8080` でバックエンドAPIが動作している必要があります
- **実行時間**: トークン数と期間により、シミュレーションに数分かかる場合があります
- **メモリ使用**: 長期間のシミュレーションでは大量のメモリを使用する可能性があります