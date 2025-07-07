# CLI Tokens - Volatility Tokens Analysis Tool

## 概要

`cli_tokens`は、高ボラティリティトークンの取得と価格予測を行うコマンドラインツールです。フロントエンドの機能をCLIとして提供し、バッチ処理や自動化に適した形で実装します。

## アーキテクチャ

### ディレクトリ構造

```
cli_tokens/
├── Cargo.toml
├── docs/
│   └── design.md
├── src/
│   ├── main.rs              # エントリポイント、CLI引数処理
│   ├── lib.rs               # ライブラリのルート
│   ├── commands/
│   │   ├── mod.rs
│   │   ├── top.rs           # topコマンドの実装
│   │   ├── predict.rs       # predictコマンドの実装
│   │   └── verify.rs        # verifyコマンドの実装
│   ├── api/
│   │   ├── mod.rs
│   │   ├── backend.rs       # バックエンドAPIクライアント
│   │   └── chronos.rs       # Chronos APIクライアント
│   ├── models/
│   │   ├── mod.rs
│   │   ├── token.rs         # トークン関連のデータ構造
│   │   ├── prediction.rs    # 予測関連のデータ構造
│   │   └── verification.rs  # 検証関連のデータ構造
│   └── utils/
│       ├── mod.rs
│       ├── file.rs          # ファイル入出力
│       ├── config.rs        # 設定管理
│       └── metrics.rs       # 精度メトリクス計算
└── tests/
    └── integration/
        ├── top_test.rs
        ├── predict_test.rs
        └── verify_test.rs
```

## ワークフロー統合

`cli_tokens`は高ボラティリティトークンの分析から予測、検証まで一連のワークフローを提供します：

```bash
# 1. 高ボラティリティトークンを取得
cli_tokens top -l 5

# 2. 価格履歴を取得
cli_tokens history tokens/wrap.near/sample.token.near.json

# 3. 実データを使用して予測実行
cli_tokens predict tokens/wrap.near/sample.token.near.json

# 4. 予測結果を検証
cli_tokens verify predictions/wrap.near/sample.token.near.json
```

各コマンドは独立して実行可能ですが、上記の順序で実行することで完全な分析パイプラインを構築できます。

## コマンド仕様

### topコマンド

高ボラティリティトークンを取得してトークン毎にファイルに保存します。

> **⚠️ 注意**: topコマンドの実行には約10分程度かかる場合があります。バックエンドAPIでの大量データ処理のため、実行中は辛抱強くお待ちください。

#### ボラティリティ計算の基準

- **Quote Token**: デフォルトで`wrap.near`を使用（`--quote-token`で変更可能）
- **価格ベース**: 各トークンの指定Quote Token建て価格の変動を分析
- **期間**: 指定された期間内での価格変動率を計算

#### 使用例

```bash
# 基本的な使用（wrap.nearベース）
cli_tokens top -l 5

# 異なるquote tokenを使用
cli_tokens top -l 5 --quote-token usdc.tether-token.near

# 特定期間でのボラティリティ分析
cli_tokens top -s 2025-06-01 -e 2025-07-01 -l 10 --quote-token wrap.near
```

#### コマンド仕様

```bash
cli_tokens top [OPTIONS]

OPTIONS:
    -s, --start <DATE>         開始日 (YYYY-MM-DD形式) [デフォルト: 30日前]
    -e, --end <DATE>           終了日 (YYYY-MM-DD形式) [デフォルト: 現在]
    -l, --limit <NUMBER>       取得するトークン数 [デフォルト: 10]
    -o, --output <DIR>         出力ディレクトリ [デフォルト: tokens/]
    -f, --format <FORMAT>      出力形式 (json|csv) [デフォルト: json]
    --quote-token <TOKEN>      ボラティリティ計算の基準トークン [デフォルト: wrap.near]
    -h, --help                 ヘルプを表示
```

#### 出力ファイル構造

```
tokens/
├── wrap.near/                     # Quote tokenディレクトリ
│   ├── sample.token.near.json     # 各トークンの詳細データ
│   ├── another.token.near.json
│   └── third.token.near.json
└── usdc.tether-token.near/        # 異なるquote tokenの例
    ├── sample.token.near.json
    └── another.token.near.json
```

#### 個別トークンファイル形式 (例: tokens/wrap.near/sample.token.near.json)

```json
{
  "metadata": {
    "generated_at": "2025-01-03T12:00:00Z",
    "start_date": "2024-12-04",
    "end_date": "2025-01-03",
    "token": "sample.token.near"
  },
  "token_data": {
    "token": "sample.token.near",
    "volatility_score": 0.85,
    "price_data": {
      "current_price": 5.23,
      "price_change_24h": 0.12,
      "volume_24h": 1234567.89
    }
  }
}
```


### historyコマンド

指定されたトークンファイルから価格履歴データを取得して保存します。predictコマンドでモックデータではなく実際のデータを使用するために必要です。

> **⚠️ 注意**: historyコマンドはバックエンドAPI (`http://localhost:8080`) が動作している必要があります。

```bash
cli_tokens history [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス (例: tokens/wrap.near.json)

OPTIONS:
    --quote-token <TOKEN>  見積りトークン（価格表示の基準） [デフォルト: wrap.near]
    -o, --output <DIR>     出力ディレクトリ [デフォルト: history/]
    --force                既存の履歴データを強制上書き
    -h, --help             ヘルプを表示
```

#### 動作仕様

1. **期間の自動検出**: トークンファイルのメタデータから`start_date`と`end_date`を自動抽出
2. **API呼び出し**: バックエンドの`/stats/get_values`エンドポイントを使用して価格履歴を取得
3. **データ保存**: 取得した価格履歴を`history/`ディレクトリに保存

#### トークンペアの概念

- **Base Token**: 価格が表示される対象トークン（トークンファイルで指定）
- **Quote Token**: 価格表示の基準通貨（`--quote-token`オプションで指定）
- 価格は「1 Base Token = X Quote Token」の形式で表現
- 例：`sample.token.near`の`wrap.near`建て価格を取得

#### 注意事項

- **データ可用性**: 指定したトークンペアと期間にデータが存在しない場合は空の結果が返されます
- **トークンペア**: 同一トークン（例: wrap.near → wrap.near）では価格データが存在しない場合があります
- **期間設定**: 十分な取引履歴がある期間を指定する必要があります

#### 出力ファイル構造

```
history/
├── wrap.near/                            # Quote tokenディレクトリ
│   ├── sample.token.near.json           # Base tokenファイル
│   └── another.token.near.json
└── usdc.tether-token.near/              # 異なるquote tokenの例
    └── sample.token.near.json
```

#### 価格履歴ファイル形式 (例: history/wrap.near/sample.token.near.json)

```json
{
  "metadata": {
    "generated_at": "2025-07-07T12:00:00Z",
    "start_date": "2025-07-06",
    "end_date": "2025-07-07",
    "base_token": "sample.token.near",
    "quote_token": "wrap.near"
  },
  "price_history": {
    "values": [
      {
        "time": "2025-07-06T00:00:00",
        "value": 5.23
      },
      {
        "time": "2025-07-06T01:00:00", 
        "value": 5.25
      }
    ]
  }
}
```

#### 使用例

```bash
# 基本的な履歴取得
cli_tokens history tokens/wrap.near/sample.token.near.json

# 異なる見積りトークンを指定
cli_tokens history tokens/usdc.tether-token.near/sample.token.near.json --quote-token usdc.tether-token.near

# 出力ディレクトリを指定
cli_tokens history tokens/wrap.near/sample.token.near.json -o custom_history/

# 既存データを上書き
cli_tokens history tokens/wrap.near/sample.token.near.json --force
```


### predictコマンド

指定されたトークンファイルに対してzeroshot予測を実行します。

```bash
cli_tokens predict [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス (例: tokens/sample.token.near.json)

OPTIONS:
    -o, --output <DIR>     出力ディレクトリ [デフォルト: predictions/]
    -m, --model <MODEL>    予測モデル [デフォルト: server_default]
                          選択肢: chronos_bolt, autogluon, statistical, server_default
    --force                既存の予測結果を強制上書き
    --start-pct <PCT>      データ範囲の開始パーセンテージ (0.0-100.0) [デフォルト: 0.0]
    --end-pct <PCT>        データ範囲の終了パーセンテージ (0.0-100.0) [デフォルト: 100.0]
    --forecast-ratio <PCT> 予測期間の比率（入力データ期間に対する%）(0.0-500.0) [デフォルト: 10.0]
    -h, --help            ヘルプを表示
```

#### データ範囲指定オプション

`--start-pct`と`--end-pct`オプションにより、予測に使用する履歴データの範囲を柔軟に指定できます：

```bash
# 全データを使用（デフォルト）
cli_tokens predict tokens/wrap.near/sample.token.near.json

# 最初の30%のデータのみ使用（バックテスト用）
cli_tokens predict tokens/wrap.near/sample.token.near.json --end-pct 30.0

# 中間期間（20%-80%）の分析
cli_tokens predict tokens/wrap.near/sample.token.near.json --start-pct 20.0 --end-pct 80.0

# 最新30%のデータのみ（最近のトレンド分析）
cli_tokens predict tokens/wrap.near/sample.token.near.json --start-pct 70.0
```

#### 予測期間指定オプション

`--forecast-ratio`オプションにより、入力データ期間に対する相対的な予測期間を指定できます：

```bash
# デフォルト（入力データ期間の10%）
cli_tokens predict tokens/wrap.near/sample.token.near.json

# 短期予測（入力データ期間の5%）
cli_tokens predict tokens/wrap.near/sample.token.near.json --forecast-ratio 5.0

# 中期予測（入力データ期間の25%）
cli_tokens predict tokens/wrap.near/sample.token.near.json --forecast-ratio 25.0

# 長期予測（入力データ期間と同じ期間）
cli_tokens predict tokens/wrap.near/sample.token.near.json --forecast-ratio 100.0
```

##### 予測期間の計算例

```
入力データ期間: 30日間
--forecast-ratio 10.0 (デフォルト) → 予測期間: 3日間
--forecast-ratio 25.0 → 予測期間: 7.5日間
--forecast-ratio 100.0 → 予測期間: 30日間

入力データ期間: 7日間
--forecast-ratio 10.0 (デフォルト) → 予測期間: 16.8時間
--forecast-ratio 50.0 → 予測期間: 3.5日間
```

##### 設計の利点

1. **データ適応的**: 入力データの期間に応じて予測期間が自動調整される
2. **一貫性**: 異なるデータセットでも一貫した比率での予測が可能
3. **直感的**: 「入力データの10%の期間で予測」のような理解しやすい指定
4. **ML的妥当性**: 訓練データ期間に対する適切な予測期間の比率設定

#### 出力ファイル構造

```
predictions/
├── wrap.near/                     # Quote tokenディレクトリ
│   ├── sample.token.near.json    # 予測結果
│   └── another.token.near.json
└── usdc.tether-token.near/       # 異なるquote tokenの例
    └── sample.token.near.json
```

### verifyコマンド

予測結果の精度を実際のデータと比較して検証します。

```bash
cli_tokens verify [OPTIONS] <PREDICTION_FILE>

ARGUMENTS:
    <PREDICTION_FILE>      予測ファイルパス (例: predictions/sample.token.near.json)

OPTIONS:
    --actual-data-file <FILE>  実データファイルパス (省略時は自動推定: tokens/{token}.json)
    -o, --output <DIR>         出力ディレクトリ [デフォルト: verification/]
    --force                    既存の検証結果を強制上書き
    -h, --help                 ヘルプを表示
```

#### 検証プロセス

1. **予測ファイル解析**: predictions/wrap.near.json から検証に必要な情報を自動抽出
   - 対象トークン名
   - 予測期間（開始〜終了タイムスタンプ）
   - 予測値リスト
2. **実データファイル自動推定**: トークン名から対応ファイルを自動特定
   - `predictions/wrap.near.json` → `tokens/wrap.near.json`
   - ファイル存在確認とトークン名の一致確認
3. **実データ取得**: 実データファイルのメタデータを基にAPIから予測期間の実データを取得
4. **データ照合**: タイムスタンプベースで予測値と実データを照合
5. **精度計算**: MAE、RMSE、MAPE、方向精度などの評価指標を算出
6. **レポート生成**: JSON形式の詳細レポートを自動生成

#### 出力ファイル構造

```
verification/
├── wrap.near/                     # Quote tokenディレクトリ
│   └── sample.token.near/         # Base tokenディレクトリ
│       └── verification_report.json  # 詳細な検証結果（メトリクス含む）
└── usdc.tether-token.near/       # 異なるquote tokenの例
    └── sample.token.near/
        └── verification_report.json
```

#### 検証レポート形式

```json
{
  "token": "sample.token.near",
  "prediction_id": "task_12345",
  "verification_date": "2025-07-06T12:00:00Z",
  "period": {
    "start": "2025-07-01T00:00:00Z",
    "end": "2025-07-05T23:59:59Z",
    "predicted_points_count": 60,
    "actual_points_count": 58,
    "matched_points_count": 58
  },
  "metrics": {
    "mae": 0.0234,
    "rmse": 0.0456,
    "mape": 2.34,
    "direction_accuracy": 0.85,
    "correlation": 0.92
  },
  "data_points": [
    {
      "timestamp": "2025-07-01T00:00:00Z",
      "predicted_value": 5.23,
      "actual_value": 5.18,
      "error": 0.05,
      "percentage_error": 0.96
    }
  ]
}
```

#### 使用例

```bash
# 基本的な検証（実データファイルは自動推定）
cli_tokens verify predictions/wrap.near/sample.token.near.json

# 実データファイルを明示的に指定
cli_tokens verify predictions/wrap.near/sample.token.near.json --actual-data-file tokens/wrap.near/sample.token.near.json

# 出力ディレクトリを指定
cli_tokens verify predictions/wrap.near/sample.token.near.json -o custom_verification/

# 既存結果を上書き
cli_tokens verify predictions/wrap.near/sample.token.near.json --force
```

#### 検証データの流れ（自動推定）

1. **予測ファイル (predictions/wrap.near/sample.token.near.json)** から：
   - トークン名 `sample.token.near` と quote token `wrap.near` を抽出
   - 予測期間と予測値を取得
2. **実データファイル自動特定**: `tokens/wrap.near/sample.token.near.json` を自動推定
   - トークン名とデータ期間を取得
   - 予測ファイルとの整合性確認
3. **API取得**: 予測期間に対応する実データを取得
4. **検証**: 予測値と実データを比較して精度を算出

#### 自動推定の利点

- **最小限の入力**: 予測ファイルのパスのみで完全な検証が可能
- **規約ベース**: ファイル名規約に従った自動化
- **エラー回避**: パス指定ミスの削減
- **柔軟性**: 必要に応じて明示的な指定も可能

## 実装詳細

### 1. フロントエンドコードの再利用戦略

#### 共有可能なコンポーネント
- **APIクライアント**: `frontend/src/chronos_api/predict.rs`の`ChronosApiClient`
- **予測ロジック**: `frontend/src/prediction_utils.rs`の予測実行・メトリクス計算機能
- **データ構造**: `frontend/src/services.rs`の`VolatilityPredictionService`のモデル定義
- **エラーハンドリング**: `frontend/src/errors.rs`の`PredictionError`

#### 依存関係の取り込み
```toml
[dependencies]
# フロントエンドの共通機能を使用
frontend = { path = "../frontend", features = ["api-client"] }
```

### 2. APIクライアント

#### Backend API (`api/backend.rs`)
- フロントエンドの`VolatilityPredictionService::get_volatility_tokens()`を活用
- `get_token_history()`: トークンの価格履歴取得

#### Chronos API (`api/chronos.rs`)
- フロントエンドの`ChronosApiClient`を直接使用
- `predict_zero_shot()`: ゼロショット予測の実行
- `poll_prediction_status()`: 予測ステータスのポーリング

### 3. データモデル

#### フロントエンドからの再利用
```rust
// フロントエンドの構造体を直接使用
pub use frontend::services::{
    VolatilityTokenResult,
    VolatilityPredictionResult,
    TokenVolatilityData,
};
pub use frontend::prediction_utils::{
    PredictionResult,
    PredictionMetrics,
    ValueAtTime,
};
pub use frontend::chronos_api::predict::{
    ZeroShotPredictionRequest,
    PredictionResponse,
};
```

#### CLI固有のモデル (`models/cli.rs`)
```rust
// CLI固有のファイル出力形式
pub struct TokenFileData {
    pub metadata: FileMetadata,
    pub token_data: VolatilityTokenResult,
}

pub struct FileMetadata {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub start_date: String,
    pub end_date: String,
    pub token: String,
}
```

#### 検証関連のモデル (`models/verification.rs`)
```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationReport {
    pub token: String,
    pub prediction_id: String,
    pub verification_date: DateTime<Utc>,
    pub period: VerificationPeriod,
    pub metrics: VerificationMetrics,
    pub data_points: Vec<ComparisonPoint>,
    pub summary: VerificationSummary,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationMetrics {
    pub mae: f64,              // Mean Absolute Error
    pub rmse: f64,             // Root Mean Square Error
    pub mape: f64,             // Mean Absolute Percentage Error
    pub direction_accuracy: f64, // 上昇/下降の予測精度
    pub correlation: f64,       // 相関係数
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComparisonPoint {
    pub timestamp: DateTime<Utc>,
    pub predicted_value: f64,
    pub actual_value: f64,
    pub error: f64,
    pub percentage_error: f64,
}
```

### 4. エラーハンドリング

```rust
// フロントエンドのエラーを基底として使用
pub use frontend::errors::PredictionError;

#[derive(Debug, thiserror::Error)]
pub enum CliTokensError {
    #[error("Prediction error: {0}")]
    Prediction(#[from] PredictionError),
    
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    
    #[error("CLI argument error: {0}")]
    CliArgument(String),
    
    #[error("File already exists: {path}. Use --force to overwrite")]
    FileExists { path: String },
}
```

### 5. 設定管理

環境変数および設定ファイルによる設定：

```toml
# ~/.config/cli_tokens/config.toml
[api]
backend_url = "http://localhost:8080"
chronos_url = "http://localhost:8000"
timeout_seconds = 300

[defaults]
volatility_limit = 10
prediction_model = "server_default"
```

## 依存関係

主要な依存クレート：
- `clap`: CLI引数パーシング
- `tokio`: 非同期ランタイム
- `reqwest`: HTTPクライアント
- `serde`, `serde_json`: シリアライゼーション
- `chrono`: 日付時刻処理
- `anyhow`, `thiserror`: エラーハンドリング
- `tracing`: ロギング
- `indicatif`: プログレスバー表示

## テスト戦略

1. **単体テスト**: 各モジュールの個別機能をテスト
2. **統合テスト**: APIクライアントとコマンド全体の動作をテスト
3. **モックサーバー**: `mockito`を使用したAPIレスポンスのモック

## 開発フロー

1. **機能追加時**:
   - フロントエンドの既存機能を確認し、再利用可能性を検討
   - CLI固有のモデルのみ`models/`に追加
   - フロントエンドのAPIクライアントを活用
   - コマンドロジックを実装
   - テストを追加

2. **ビルドとテスト**:
   ```bash
   cargo build
   cargo test
   cargo clippy
   cargo fmt
   ```

3. **実行例**:
   ```bash
   # 高ボラティリティトークンTop10を取得
   cargo run -- top -l 10 -o tokens/
   
   # 特定のトークンに対して予測を実行
   cargo run -- predict tokens/wrap.near/sample.token.near.json -o predictions/
   
   # 予測結果を検証
   cargo run -- verify predictions/wrap.near/sample.token.near.json
   ```

## パフォーマンス考慮事項

1. **APIレスポンス**: 適切なキャッシング機能
2. **レート制限**: API呼び出しのレート制限対応
3. **メモリ効率**: 大量のトークンデータを効率的に処理
4. **ファイル処理**: 個別ファイルによる効率的なデータ管理

## セキュリティ

1. **API認証**: 必要に応じてAPIキーの安全な管理
2. **入力検証**: ユーザー入力の適切な検証
3. **エラー情報**: センシティブな情報を含まないエラーメッセージ