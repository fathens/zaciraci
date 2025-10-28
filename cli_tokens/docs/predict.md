# CLI Tokens - Volatility Tokens Analysis Tool

> **注意**: このファイルは1125行と非常に長いため、将来的にコマンド別のファイルに分割予定です。
> - 概要情報は [README.md](./README.md) を参照してください
> - 分割予定: top.md, history.md, predict.md, verify.md, report.md, chart.md

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
│   │   ├── verify.rs        # verifyコマンドの実装
│   │   ├── simulate.rs      # simulateコマンドの実装
│   │   └── report.rs        # reportコマンドの実装
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
# 作業ディレクトリを設定
export CLI_TOKENS_BASE_DIR="./workspace"

# 1. 高ボラティリティトークンを取得
cli_tokens top -l 5 --output tokens

# 2. 価格履歴を取得
cli_tokens history tokens/wrap.near/sample.token.near.json --output history

# 3. 価格予測タスクを開始（非同期実行）
cli_tokens predict kick tokens/wrap.near/sample.token.near.json --output predictions

# 4. 予測結果を取得（完了まで待機）
cli_tokens predict pull tokens/wrap.near/sample.token.near.json --output predictions

# 5. 予測結果を検証
cli_tokens verify predictions/wrap.near/sample.token.near.json --output verification
```

各コマンドは独立して実行可能ですが、上記の順序で実行することで完全な分析パイプラインを構築できます。

## コマンド仕様

### topコマンド

高ボラティリティトークンの情報を取得してトークン毎にファイルに保存します。

> **⚠️ 注意**: topコマンドの実行には数分程度かかる場合があります。バックエンドAPIでのボラティリティ計算処理のため、実行中は辛抱強くお待ちください。

#### 出力内容

- **トークン情報**: 高ボラティリティトークンの基本情報
- **メタデータ**: 生成日時、対象期間、Quote Token情報
- **シンプル形式**: 軽量なJSON構造でトークン識別情報のみ
- **Quote Token**: デフォルトで`wrap.near`を使用（`--quote-token`で変更可能）

#### 使用例

```bash
# 環境変数を設定して作業ディレクトリを統一
export CLI_TOKENS_BASE_DIR="./workspace"

# 基本的な使用（wrap.nearベース）
cli_tokens top -l 5 --output tokens

# 異なるquote tokenを使用
cli_tokens top -l 5 --quote-token usdc.tether-token.near --output tokens

# 特定期間でのボラティリティ分析
cli_tokens top -s 2025-06-01 -e 2025-07-01 -l 10 --quote-token wrap.near --output tokens

# 最小深度フィルターを指定
cli_tokens top -l 5 --min-depth 500000 --output tokens
```

#### コマンド仕様

```bash
cli_tokens top [OPTIONS]

OPTIONS:
    -s, --start <DATE>         開始日 (YYYY-MM-DD形式) [デフォルト: 30日前]
    -e, --end <DATE>           終了日 (YYYY-MM-DD形式) [デフォルト: 現在]
    -l, --limit <NUMBER>       取得するトークン数 [デフォルト: 10]
    -o, --output <DIR>         出力ディレクトリ [デフォルト: tokens/] ※CLI_TOKENS_BASE_DIRからの相対パス
    -f, --format <FORMAT>      出力形式 (json|csv) [デフォルト: json]
    --quote-token <TOKEN>      ボラティリティ計算の基準トークン [デフォルト: wrap.near]
    --min-depth <NUMBER>       最小深度フィルター [デフォルト: 1000000]
    -h, --help                 ヘルプを表示
```

#### 出力ファイル構造

環境変数`CLI_TOKENS_BASE_DIR`で指定したディレクトリ配下に以下の構造で保存されます：

```
${CLI_TOKENS_BASE_DIR}/
└── tokens/
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
    <TOKEN_FILE>           トークンファイルパス (例: tokens/wrap.near/sample.token.near.json)

OPTIONS:
    --quote-token <TOKEN>  見積りトークン（価格表示の基準） [デフォルト: wrap.near]
    -o, --output <DIR>     出力ディレクトリ [デフォルト: price_history/] ※CLI_TOKENS_BASE_DIRからの相対パス
    --force                既存の履歴データを強制上書き
    -h, --help             ヘルプを表示
```

#### 使用例

```bash
# 環境変数を設定
export CLI_TOKENS_BASE_DIR="./workspace"

# 基本的な使用（トークンファイルから履歴を取得）
cli_tokens history tokens/wrap.near/sample.token.near.json --output price_history

# 異なるquote tokenで履歴を取得
cli_tokens history tokens/wrap.near/sample.token.near.json --quote-token usdc.tether-token.near --output price_history
```

#### 動作仕様

1. **期間の自動検出**: トークンファイルのメタデータから`start_date`と`end_date`を自動抽出
2. **API呼び出し**: バックエンドの`/stats/get_values`エンドポイントを使用して価格履歴を取得
3. **データ保存**: 取得した価格履歴を`price_history/`ディレクトリに保存（期間情報を含むファイル名で）

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
${CLI_TOKENS_BASE_DIR}/
└── price_history/
    └── {quote_token}/
        └── {base_token}/
            └── history-{start}-{end}.json    # 期間を含むファイル名
```

例：
```
${CLI_TOKENS_BASE_DIR}/
└── price_history/
    ├── wrap.near/
    │   └── sample.token.near/
    │       ├── history-20250801_0000-20250807_2359.json
    │       └── history-20250815_1200-20250820_1200.json
    └── usdc.tether-token.near/
        └── sample.token.near/
            └── history-20250801_0000-20250807_2359.json
```

#### 価格履歴ファイル形式 (例: price_history/wrap.near/sample.token.near/history-20250706_0000-20250707_2359.json)

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
cli_tokens history tokens/wrap.near/sample.token.near.json -o price_history/

# 既存データを上書き
cli_tokens history tokens/wrap.near/sample.token.near.json --force
```


### predictコマンド

predictコマンドは、指定されたトークンファイルに対してzeroshot予測を実行します。長時間実行される予測タスクを効率的に管理するため、`kick`と`pull`の2つのサブコマンドに分かれています。

> **⚠️ 注意**: predictコマンドは機械学習モデルの訓練を含むため、実行時間が非常に長くなります（通常30分〜2時間程度）。Chronos API (`http://localhost:8000`) が動作している必要があります。

#### predictサブコマンドの構造

```bash
cli_tokens predict <SUBCOMMAND>

SUBCOMMANDS:
    kick    非同期予測タスクを開始
    pull    予測タスクの結果を取得
```

#### kickサブコマンド

非同期予測タスクをChronos APIに送信し、即座に終了します。

```bash
cli_tokens predict kick [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス (例: tokens/wrap.near/sample.token.near.json)

OPTIONS:
    -o, --output <DIR>     出力ディレクトリ [デフォルト: predictions/] ※CLI_TOKENS_BASE_DIRからの相対パス
    -m, --model <MODEL>    予測モデル（指定しない場合はサーバー側のデフォルトモデルを使用）
                          利用可能なモデルについては下記「指定可能なモデル一覧」を参照
    --start-pct <PCT>      データ範囲の開始パーセンテージ (0.0-100.0) [デフォルト: 0.0]
    --end-pct <PCT>        データ範囲の終了パーセンテージ (0.0-100.0) [デフォルト: 100.0]
    --forecast-ratio <PCT> 予測期間の比率（入力データ期間に対する%）(0.0-500.0) [デフォルト: 10.0]
    -h, --help            ヘルプを表示
```

**動作**：
1. トークンファイルと履歴データを読み込み
2. 指定されたパラメータで予測リクエストを作成
3. Chronos APIに非同期予測タスクを送信（**API Path: `POST /api/v1/predict_zero_shot_async`**）
4. 返されたtask_idを含むタスク情報を`${output}/${quote_token}/${base_token}.task.json`に保存
5. 即座に終了（ポーリングなし）

**タスクファイルの形式**：
```json
{
  "task_id": "550e8400-e29b-41d4-a716-446655440000",
  "created_at": "2024-01-01T10:00:00Z",
  "token_file": "tokens/wrap.near/usdc.tether-token.near.json",
  "model": "chronos-small",
  "params": {
    "start_pct": 0.0,
    "end_pct": 100.0,
    "forecast_ratio": 10.0
  },
  "last_status": "pending",
  "last_checked_at": null,
  "poll_count": 0
}
```

#### 指定可能なモデル一覧

予測に使用できるモデルは以下の通りです。モデルの詳細情報は、Chronos APIの `/api/v1/models` エンドポイントから取得できます。

**APIエンドポイント**:
```bash
curl http://localhost:8000/api/v1/models
```

**利用可能なモデル**:

| モデル名 | 説明 | 処理時間 | 推奨用途 |
|---------|------|----------|----------|
| `chronos_default` | デフォルトの時系列予測モデル | 15分以内 | 一般的な予測 |
| `fast_statistical` | 統計的手法中心・高速モデル | 5分以内 | 短期予測・リアルタイム処理 |
| `balanced_ml` | 機械学習ベース・バランス型モデル | 15分以内 | 中期予測・バランス重視 |
| `deep_learning` | 深層学習ベース・高精度モデル | 30分以内 | 長期予測・最高精度 |
| `autoets_only` | AutoETSモデルのみ（統計的手法・高速） | 5分以内 | 短期予測・統計的手法 |
| `npts_only` | NPTSモデルのみ（非パラメトリック時系列） | 10分以内 | 中期予測・非パラメトリック手法 |
| `seasonal_naive_only` | SeasonalNaiveモデルのみ（季節性ベースライン） | 1分以内 | ベースライン予測・超高速処理 |
| `recursive_tabular_only` | RecursiveTabularモデルのみ（勾配ブースティング） | 15分以内 | 中長期予測・勾配ブースティング |
| `ets_only` | ETSモデルのみ（統計的手法・標準） | 5分以内 | 短期予測・統計的手法 |
| `chronos_zero_shot` | Chronos事前訓練済みTransformer | 5分以内 | 即座予測・Zero Shot予測 |

**使用例**:
```bash
# デフォルトモデルを使用
cli_tokens predict kick tokens/wrap.near/sample.token.near.json

# 高速統計モデルを使用
cli_tokens predict kick tokens/wrap.near/sample.token.near.json --model fast_statistical

# 深層学習モデルを使用（高精度だが時間がかかる）
cli_tokens predict kick tokens/wrap.near/sample.token.near.json --model deep_learning
```

#### pullサブコマンド

保存されたタスク情報を読み込み、予測結果を取得します。

```bash
cli_tokens predict pull [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス (例: tokens/wrap.near/sample.token.near.json)

OPTIONS:
    -o, --output <DIR>     タスクファイルと結果の保存先 [デフォルト: predictions/]
    --max-polls <NUM>      最大ポーリング回数 [デフォルト: 30]
    --poll-interval <SEC>  ポーリング間隔（秒）[デフォルト: 2]
    -h, --help            ヘルプを表示
```

**動作**：
1. `${output}/${quote_token}/${base_token}.task.json`からタスク情報を読み込み
2. task_idを使用してChronos APIに対してポーリング
3. ステータスに応じた処理：
   - `completed`: 結果を取得して`${output}/${quote_token}/${base_token}.json`に保存
   - `running`/`pending`: 指定間隔で再ポーリング
   - `failed`: エラーメッセージを表示
4. 最大ポーリング回数に達した場合はタイムアウト

#### 使用例

```bash
# 環境変数を設定
export CLI_TOKENS_BASE_DIR="./workspace"

# 基本的な使用方法
## 1. タスクを開始
cli_tokens predict kick tokens/wrap.near/sample.token.near.json

## 2. 結果を取得（完了まで待機）
cli_tokens predict pull tokens/wrap.near/sample.token.near.json

# 複数モデルでの並列実行
## 異なるモデルで予測を開始
cli_tokens predict kick tokens/wrap.near/sample.token.near.json \
  --model chronos-small --output predictions/small

cli_tokens predict kick tokens/wrap.near/sample.token.near.json \
  --model chronos-large --output predictions/large

## それぞれの結果を取得
cli_tokens predict pull tokens/wrap.near/sample.token.near.json \
  --output predictions/small

cli_tokens predict pull tokens/wrap.near/sample.token.near.json \
  --output predictions/large

# 異なるパラメータでの実験
## 時間範囲を変えて実験
cli_tokens predict kick tokens/wrap.near/sample.token.near.json \
  --start-pct 0 --end-pct 50 --output predictions/first-half

cli_tokens predict kick tokens/wrap.near/sample.token.near.json \
  --start-pct 50 --end-pct 100 --output predictions/second-half

## 結果を取得
cli_tokens predict pull tokens/wrap.near/sample.token.near.json \
  --output predictions/first-half --max-polls 60
```

#### データ範囲指定オプション

`--start-pct`と`--end-pct`オプションにより、予測に使用する履歴データの範囲を時刻ベースで柔軟に指定できます：

##### 時刻ベースのパーセンテージ計算

パーセンテージは、データの最初と最後のタイムスタンプ間の時間軸に基づいて計算されます：

- 0%: 最初のデータポイントの時刻
- 100%: 最後のデータポイントの時刻
- 指定されたパーセンテージは、この時間範囲内の相対的な位置を表します

例：データが2025-01-01 00:00から2025-01-11 00:00まで（10日間）の場合
- --start-pct 20.0: 2025-01-03 00:00以降のデータ（開始から2日後）
- --end-pct 80.0: 2025-01-09 00:00以前のデータ（開始から8日後）

```bash
# 全データを使用（デフォルト）
cli_tokens predict tokens/wrap.near/sample.token.near.json

# 最初の30%の期間のデータのみ使用（バックテスト用）
cli_tokens predict tokens/wrap.near/sample.token.near.json --end-pct 30.0

# 中間期間（20%-80%）の分析
cli_tokens predict tokens/wrap.near/sample.token.near.json --start-pct 20.0 --end-pct 80.0

# 最新30%の期間のデータのみ（最近のトレンド分析）
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
${CLI_TOKENS_BASE_DIR}/
└── predictions/
    └── {model_name}[_{params_hash}]/
        └── {quote_token}/
            └── {base_token}/
                └── history-{hist_start}-{hist_end}/
                    └── predict-{pred_start}-{pred_end}.json
```

例：
```
${CLI_TOKENS_BASE_DIR}/
└── predictions/
    ├── chronos_default/
    │   └── wrap.near/
    │       └── sample.token.near/
    │           └── history-20250801_0000-20250807_2359/
    │               ├── predict-20250808_0000-20250809_0000.json
    │               └── predict-20250808_1200-20250809_1200.json
    └── chronos_zero_shot/
        └── wrap.near/
            └── sample.token.near/
                └── history-20250801_0000-20250807_2359/
                    └── predict-20250808_0000-20250809_0000.json
```

注：タスク情報は一時ファイルとして別途管理されます。

#### kick/pullサブコマンドの設計上の利点

1. **非同期実行**: 長時間実行される予測タスクをバックグラウンドで処理
2. **並列実験**: 異なるモデルやパラメータで同時に複数の予測を実行
3. **柔軟な管理**: outputディレクトリごとに独立したタスク管理
4. **競合状態の回避**: 各タスクが独立したファイルを使用
5. **中断・再開可能**: pullコマンドでいつでも結果を取得可能

### verifyコマンド

予測結果の精度を実際のデータと比較して検証します。

```bash
cli_tokens verify [OPTIONS] <PREDICTION_FILE>

ARGUMENTS:
    <PREDICTION_FILE>      予測ファイルパス (例: predictions/wrap.near/sample.token.near.json)

OPTIONS:
    --actual-data-file <FILE>  実データファイルパス (省略時は自動推定: tokens/{quote_token}/{token}.json)
    -o, --output <DIR>         出力ディレクトリ [デフォルト: verification/] ※CLI_TOKENS_BASE_DIRからの相対パス
    --force                    既存の検証結果を強制上書き
    -h, --help                 ヘルプを表示
```

#### 使用例

```bash
# 環境変数を設定
export CLI_TOKENS_BASE_DIR="./workspace"

# 基本的な検証（実データファイルを自動推定）
cli_tokens verify predictions/wrap.near/sample.token.near.json --output verification

# 実データファイルを明示的に指定
cli_tokens verify predictions/wrap.near/sample.token.near.json \
  --actual-data-file tokens/wrap.near/sample.token.near.json \
  --output verification

# 検証結果を強制上書き
cli_tokens verify predictions/wrap.near/sample.token.near.json --force --output verification
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
${CLI_TOKENS_BASE_DIR}/
└── verification/
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

### simulateコマンド

実際の価格データを使用してトレーディングアルゴリズムのバックテストを実行し、パフォーマンス分析を行います。3つのアルゴリズム（momentum、portfolio、trend_following）に対応しています。

詳細は[simulate.md](./simulate.md)を参照してください。

```bash
# 基本的な使用例（topコマンドでトークン情報を事前取得）
cli_tokens top --start 2024-11-01 --end 2024-12-01 --limit 5
cli_tokens simulate --start 2024-12-01 --end 2024-12-31 --algorithm momentum
```

### reportコマンド

シミュレーション結果のJSONファイルからHTMLレポートを生成します。Chart.jsを使用したインタラクティブなチャート表示に対応しています。

詳細は[simulate.md](./simulate.md#reportコマンド)を参照してください。

```bash
# 基本的な使用例
cli_tokens report simulation_results/momentum_2024-12-01_2024-12-31/results.json
```

### chartコマンド

履歴データ（history）と予測データ（prediction）を組み合わせてPNGチャートを生成するコマンドです。自動検出方式を採用し、トークンファイルを起点として関連データファイルを自動で発見・統合してビジュアライゼーションを行います。

#### 基本設計思想

**自動検出方式**
- トークンファイル（例: `tokens/wrap.near/usdc.tether-token.near.json`）を起点とする
- `extract_quote_token_from_path` パターンを使用してquote_tokenを抽出
- 既存のファイル構造に従って履歴・予測ファイルを自動検索
- 見つかったデータに応じて最適なチャート形式を自動選択

**ファイル検索パターン**
```
基点: tokens/{quote_token}/{base_token}.json
履歴: price_history/{quote_token}/{base_token}/history-{start}-{end}.json  
予測: predictions/{model_name}/{quote_token}/{base_token}/history-{hist_start}-{hist_end}/predict-{pred_start}-{pred_end}.json
```

#### コマンド仕様

```bash
cli_tokens chart [OPTIONS] <TOKEN_FILE>

ARGUMENTS:
    <TOKEN_FILE>           トークンファイルパス（起点）

OPTIONS:
    -o, --output <DIR>         出力ディレクトリ [デフォルト: charts/] ※CLI_TOKENS_BASE_DIRからの相対パス
    --base-dir <DIR>           ベースディレクトリ（CLI_TOKENS_BASE_DIRをオーバーライド）
    --size <SIZE>              画像サイズ (WIDTHxHEIGHT) [デフォルト: 1200x800]
    --chart-type <TYPE>        チャートタイプ [デフォルト: auto]
                              選択肢: auto, history, prediction, combined
    --history-only             予測データの検索と描画を無効化
    --show-confidence          信頼区間を表示（予測データがある場合）
    --output-name <NAME>       出力ファイル名を明示指定
    --force                    既存ファイルを強制上書き
    -v, --verbose              詳細ログ出力
    -h, --help                 ヘルプを表示
```

#### チャートタイプ

- **`auto`** (デフォルト): 自動検出（履歴+予測があれば両方、なければ履歴のみ）
- **`history`**: 履歴データのみ
- **`prediction`**: 予測データのみ（履歴は背景として薄く表示）
- **`combined`**: 履歴と予測を重ねて表示

#### 使用例

```bash
# 環境変数を設定
export CLI_TOKENS_BASE_DIR="./workspace"

# 基本的な使用（自動検出）
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json

# 履歴データのみ
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json --chart-type history

# 予測データのみ  
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json --chart-type prediction

# 信頼区間付きで組み合わせ表示
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json --chart-type combined --show-confidence

# カスタム出力名・サイズ
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json \
  --output-name my_custom_chart \
  --size 1920x1080

# 詳細ログ付きで実行
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json --verbose

# カスタムベースディレクトリを指定
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json \
  --base-dir /custom/data/path

# 履歴のみ強制（予測があっても無視）
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json \
  --history-only

# 既存ファイルを強制上書き
cli_tokens chart tokens/wrap.near/usdc.tether-token.near.json \
  --force
```

#### 自動検出ロジック

```rust
async fn detect_data_files(token_file: &Path, base_dir: Option<&Path>) -> Result<DetectedFiles> {
    // 1. トークンファイルから情報を抽出
    let token_data: TokenFileData = load_token_file(token_file).await?;
    let quote_token = extract_quote_token_from_path(token_file)
        .or_else(|| token_data.metadata.quote_token.clone())
        .unwrap_or_else(|| "wrap.near".to_string());
    
    let base_dir = base_dir
        .map(|p| p.to_path_buf())
        .or_else(|| std::env::var("CLI_TOKENS_BASE_DIR").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));

    let token_name = sanitize_filename(&token_data.token_data.token);
    let quote_dir = sanitize_filename(&quote_token);

    // 2. 履歴ファイルを検索（最新のファイルを選択）
    let history_dir = base_dir
        .join("price_history")
        .join(&quote_dir)
        .join(&token_name);
    
    // history-YYYYMMDD_HHMM-YYYYMMDD_HHMM.json 形式のファイルを検索
    let history_file = fs::read_dir(&history_dir)
        .ok()
        .and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().extension() == Some("json".as_ref()))
                .filter(|e| e.file_name().to_string_lossy().starts_with("history-"))
                .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
                .map(|e| e.path())
        });

    // 3. 予測ファイルを検索（最新のファイルを選択）
    // predictions/{model_name}/{quote_token}/{base_token}/history-*/predict-*.json
    let predictions_root = base_dir.join("predictions");
    let prediction_file = find_latest_prediction_file(&predictions_root, &quote_dir, &token_name)?;

    Ok(DetectedFiles {
        history: if history_file.exists() { Some(history_file) } else { None },
        prediction: if prediction_file.exists() { Some(prediction_file) } else { None },
        token_name,
        quote_token,
    })
}
```

#### 出力ファイル構造

```
${CLI_TOKENS_BASE_DIR}/
└── charts/
    ├── wrap.near/
    │   ├── usdc.tether-token.near_history.png
    │   ├── usdc.tether-token.near_prediction.png
    │   ├── usdc.tether-token.near_combined.png
    │   └── usdc.tether-token.near_combined_with_confidence.png
    └── {other_quote_tokens}/
        └── ...
```

#### 出力ファイル名パターン

- `{token}_history.png`: 履歴データのみ
- `{token}_prediction.png`: 予測データのみ
- `{token}_prediction_with_confidence.png`: 信頼区間付き予測
- `{token}_combined.png`: 履歴+予測の組み合わせ
- `{token}_combined_with_confidence.png`: 信頼区間付き組み合わせ
- カスタム名指定時: `{custom_name}.png`

#### 技術仕様

**使用ライブラリ**: Plotters (推奨)
- Pure Rust実装
- PNG出力サポート（`BitMapBackend`）
- 軽量で依存関係が少ない
- 既存プロジェクトとの整合性

**チャート要素**:
- **履歴データ**: 青色の実線
- **予測データ**: 赤色の点線
- **信頼区間**: グレーの塗りつぶし領域
- **タイトル**: `{base_token} / {quote_token} Price Chart`
- **軸ラベル**: 
  - X軸: 時間 (YYYY-MM-DD)
  - Y軸: 価格 ({quote_token})

#### エラーハンドリング

- **`NoDataFound`**: 履歴・予測データが見つからない場合
- **`HistoryNotFound`**: 履歴ファイルが見つからない場合（chart-type: history指定時）
- **`PredictionNotFound`**: 予測ファイルが見つからない場合（chart-type: prediction指定時）
- **`InvalidTokenFile`**: トークンファイルが無効な場合
- **`OutputError`**: チャート生成・保存エラー

#### 実装段階

**Phase 1**: 基礎実装
- 基本的なコマンド構造とオプション解析
- ファイル自動検出ロジック
- 履歴データのみのチャート生成（Plotters使用）

**Phase 2**: 予測データ統合
- 予測データの読み込みと統合
- 履歴+予測の組み合わせ表示
- チャートタイプの完全実装

**Phase 3**: 高度な機能
- 信頼区間の表示
- カスタマイズオプション（色、スタイル等）
- エラーハンドリングの充実

**Phase 4**: 最適化・拡張
- パフォーマンス最適化
- 追加のチャート形式サポート
- バッチ処理モード

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

#### 環境変数

`CLI_TOKENS_BASE_DIR`: 作業ディレクトリのベースパスを指定します。

```bash
# 基本的な使用例
export CLI_TOKENS_BASE_DIR="/path/to/workspace"
cli_tokens top --output tokens
# → ファイルは /path/to/workspace/tokens/ に保存される

# 設定しない場合は現在のディレクトリが使用される
cli_tokens top --output tokens
# → ファイルは ./tokens/ に保存される
```

この環境変数により：
- すべてのコマンドの出力先ディレクトリが統一される
- predictコマンドが適切なhistoryファイルを自動発見できる
- 複数のプロジェクトでの作業ディレクトリ分離が可能

#### 設定ファイル

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