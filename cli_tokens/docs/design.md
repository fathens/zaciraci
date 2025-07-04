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
│   │   └── predict.rs       # predictコマンドの実装
│   ├── api/
│   │   ├── mod.rs
│   │   ├── backend.rs       # バックエンドAPIクライアント
│   │   └── chronos.rs       # Chronos APIクライアント
│   ├── models/
│   │   ├── mod.rs
│   │   ├── token.rs         # トークン関連のデータ構造
│   │   └── prediction.rs    # 予測関連のデータ構造
│   └── utils/
│       ├── mod.rs
│       ├── file.rs          # ファイル入出力
│       └── config.rs        # 設定管理
└── tests/
    └── integration/
        ├── top_test.rs
        └── predict_test.rs
```

## コマンド仕様

### topコマンド

高ボラティリティトークンを取得してトークン毎にファイルに保存します。

```bash
cli_tokens top [OPTIONS]

OPTIONS:
    -s, --start <DATE>     開始日 (YYYY-MM-DD形式) [デフォルト: 30日前]
    -e, --end <DATE>       終了日 (YYYY-MM-DD形式) [デフォルト: 現在]
    -l, --limit <NUMBER>   取得するトークン数 [デフォルト: 10]
    -o, --output <DIR>     出力ディレクトリ [デフォルト: tokens/]
    -f, --format <FORMAT>  出力形式 (json|csv) [デフォルト: json]
    -h, --help             ヘルプを表示
```

#### 出力ファイル構造

```
tokens/
├── wrap.near.json        # 各トークンの詳細データ
├── token2.near.json
└── token3.near.json
```

#### 個別トークンファイル形式 (例: wrap.near.json)

```json
{
  "metadata": {
    "generated_at": "2025-01-03T12:00:00Z",
    "start_date": "2024-12-04",
    "end_date": "2025-01-03",
    "token": "wrap.near"
  },
  "token_data": {
    "token": "wrap.near",
    "volatility_score": 0.85,
    "price_data": {
      "current_price": 5.23,
      "price_change_24h": 0.12,
      "volume_24h": 1234567.89
    }
  }
}
```


### predictコマンド

指定されたトークンファイルに対してzeroshot予測を実行します。

```bash
cli_tokens predict [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス (例: tokens/wrap.near.json)

OPTIONS:
    -o, --output <DIR>     出力ディレクトリ [デフォルト: predictions/]
    -m, --model <MODEL>    予測モデル [デフォルト: server_default]
                          選択肢: chronos_bolt, autogluon, statistical, server_default
    --force                既存の予測結果を強制上書き
    -h, --help            ヘルプを表示
```

#### 出力ファイル構造

```
predictions/
├── wrap.near/
│   ├── prediction.json   # 予測結果
│   ├── chart.svg        # 予測チャート
│   └── metrics.json     # 詳細メトリクス
```

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
   cargo run -- predict tokens/wrap.near.json -o predictions/
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