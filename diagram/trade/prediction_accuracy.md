# 予測精度トラッキングシステム

## 概要
Chronos AI による価格予測の精度を自動的にトラッキングし、ポートフォリオ最適化にフィードバックする。
予測時に prediction_records テーブルへ INSERT し、24時間後の次回実行時に実績価格と比較して精度指標（MAPE）を算出する。
算出された rolling MAPE は prediction_confidence に変換され、Sharpe/RP ブレンド係数（alpha）を動的に調整する。

## データベース設計

### prediction_records テーブル
予測と実績の対応を記録するテーブル。INSERT 時は予測情報のみ、24h 後の UPDATE で実績・精度指標を追記する。

```sql
CREATE TABLE prediction_records (
    id SERIAL PRIMARY KEY,
    evaluation_period_id VARCHAR NOT NULL,
    token VARCHAR NOT NULL,
    quote_token VARCHAR NOT NULL,
    predicted_price NUMERIC NOT NULL,
    prediction_time TIMESTAMP NOT NULL,
    target_time TIMESTAMP NOT NULL,
    actual_price NUMERIC,
    mape DOUBLE PRECISION,
    absolute_error NUMERIC,
    evaluated_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

全ての価格カラムは **TokenPrice (NEAR/token)** 単位で保存する。
`token_rates` の `rate` (yocto スケール) とは異なるので注意。

## インデックス設計

```sql
CREATE INDEX idx_prediction_records_target ON prediction_records(target_time);
CREATE INDEX idx_prediction_records_evaluated ON prediction_records(evaluated_at);
```

- `target_time`: 未評価レコードの検索（`WHERE evaluated_at IS NULL AND target_time <= NOW()`）
- `evaluated_at`: 直近の評価済みレコード取得（`ORDER BY evaluated_at DESC LIMIT N`）

## テーブル関係図

```
┌─────────────────────────┐
│   evaluation_periods    │
├─────────────────────────┤
│ period_id (VARCHAR) PK  │──────┐
│ start_time              │      │
│ initial_value           │      │ evaluation_period_id で参照
│ selected_tokens         │      │ （FK 制約なし、論理的な関係）
└─────────────────────────┘      │
                                 │
┌────────────────────────────────┴──────────────────────────────────┐
│         prediction_records                                        │
├───────────────────────────────────────────────────────────────────┤
│ id (SERIAL) PK                                                    │
│ evaluation_period_id (VARCHAR) ← evaluation_periods.period_id     │
│ token (VARCHAR) ← token_rates.base_token と同じ形式               │
│ quote_token (VARCHAR) ← "wrap.near" 固定                          │
│ predicted_price (NUMERIC) ← Chronos 予測価格 (TokenPrice)         │
│ prediction_time (TIMESTAMP) ← 予測実行時刻                        │
│ target_time (TIMESTAMP) ← prediction_time + 24h                   │
│ actual_price (NUMERIC) ← 実績価格 (TokenPrice、24h後にUPDATE)     │
│ mape (DOUBLE PRECISION) ← |pred-actual|/actual*100 (24h後)       │
│ absolute_error (NUMERIC) ← |pred-actual| (24h後)                 │
│ evaluated_at (TIMESTAMP) ← 評価実行時刻 (24h後)                   │
│ created_at (TIMESTAMP) ← DEFAULT CURRENT_TIMESTAMP                │
└───────────────────────────────────────────────────────────────────┘
         │                                         ▲
         │ token + quote_token +                   │ actual_price の
         │ target_time で検索                       │ データソース
         ▼                                         │
┌─────────────────────────┐                        │
│      token_rates        │                        │
├─────────────────────────┤                        │
│ base_token (VARCHAR)    │ ← prediction_records.token と一致       │
│ quote_token (VARCHAR)   │ ← prediction_records.quote_token と一致 │
│ rate (NUMERIC)          │──→ ExchangeRate.to_price() → actual_price
│ decimals (SMALLINT)     │                        │
│ timestamp (TIMESTAMP)   │ ← target_time ± 30分 で検索             │
└─────────────────────────┘
```

## データフロー図

```
[Chronos AI]                    [REF Finance Blockchain]
     │                                    │
     │ predicted_price                    │ actual pool state
     │ (TokenPrice: NEAR/token)           │
     ▼                                    ▼
┌─────────────────┐              ┌─────────────────┐
│ prediction_      │   24h 後     │ token_rates     │
│ records         │◄────────────│                 │
│                 │   actual =    │ rate (yocto)    │
│ INSERT:         │   to_price()  │ + decimals      │
│  predicted_price│              │                 │
│  prediction_time│              │ timestamp       │
│  target_time    │              │ (15min interval) │
│                 │              └─────────────────┘
│ UPDATE:         │
│  actual_price   │
│  mape           │
│  absolute_error │
│  evaluated_at   │
└─────────────────┘
         │
         │ 直近 N 件の evaluated レコード
         ▼
    rolling MAPE (%)
         │
         │ mape_to_confidence()
         │ MAPE ≤ 5% → 1.0, ≥ 20% → 0.0 (線形補間)
         ▼
    prediction_confidence [0.0, 1.0]
         │
         │ PortfolioData.prediction_confidence
         ▼
┌─────────────────────────────────┐
│ execute_portfolio_optimization  │
│                                 │
│ alpha_vol: ボラティリティ由来   │
│   0.7 (高ボラ) → 0.9 (低ボラ)  │
│                                 │
│ alpha = floor + (alpha_vol      │
│         - floor) * confidence   │
│                                 │
│ floor = 0.5 (PREDICTION_ALPHA_ │
│               FLOOR)            │
│                                 │
│ confidence=1.0 → alpha=alpha_vol│
│ confidence=0.0 → alpha=0.5     │
│ None → alpha=alpha_vol (後方互換)│
└─────────────────────────────────┘
```

## カラム詳細

| カラム | 型 | NULL | INSERT 時 | UPDATE 時 | 説明 |
|--------|------|------|-----------|-----------|------|
| `id` | SERIAL | NOT NULL | 自動採番 | - | 主キー |
| `evaluation_period_id` | VARCHAR | NOT NULL | 設定 | - | 評価期間 ID |
| `token` | VARCHAR | NOT NULL | 設定 | - | 予測対象トークン |
| `quote_token` | VARCHAR | NOT NULL | 設定 | - | `"wrap.near"` 固定 |
| `predicted_price` | NUMERIC | NOT NULL | 設定 | - | Chronos 予測価格 (TokenPrice) |
| `prediction_time` | TIMESTAMP | NOT NULL | 設定 | - | 予測実行時刻 |
| `target_time` | TIMESTAMP | NOT NULL | 設定 | - | `prediction_time + 24h` |
| `actual_price` | NUMERIC | NULL | NULL | 設定 | 実績価格 (TokenPrice) |
| `mape` | DOUBLE PRECISION | NULL | NULL | 設定 | `|pred-actual|/actual*100` (%) |
| `absolute_error` | NUMERIC | NULL | NULL | 設定 | `|pred-actual|` |
| `evaluated_at` | TIMESTAMP | NULL | NULL | 設定 | 評価実行時刻 |
| `created_at` | TIMESTAMP | NOT NULL | DEFAULT | - | レコード作成日時 |

## レコードのライフサイクル

```
状態 1: INSERT 直後
  predicted_price = 1.500000     ← Chronos 予測
  prediction_time = 2026-01-27 00:00:00
  target_time = 2026-01-28 00:00:00
  actual_price = NULL            ← 未評価
  mape = NULL                    ← 未評価
  absolute_error = NULL          ← 未評価
  evaluated_at = NULL            ← 未評価

状態 2: UPDATE 後（24h 経過）
  actual_price = 1.450000        ← token_rates から取得・変換
  mape = 3.45                    ← |1.5 - 1.45| / 1.45 * 100
  absolute_error = 0.050000      ← |1.5 - 1.45|
  evaluated_at = 2026-01-28 00:00:05

状態 3: DELETE（保持期間経過後）
  評価済みレコード: evaluated_at から 30 日以上経過 → 削除
  未評価レコード: target_time から 20 日以上経過 → 削除
```

## レコードのクリーンアップ

`evaluate_pending_predictions()` の評価完了後に `cleanup_old_records()` が呼び出され、古いレコードを自動削除する。

```
evaluate_pending_predictions()
    ├── 未評価レコードを評価
    ├── cleanup_old_records()  ← 評価後に呼び出し
    │   ├── 評価済み: evaluated_at < NOW - RETENTION_DAYS → DELETE
    │   └── 未評価: target_time < NOW - UNEVALUATED_RETENTION_DAYS → DELETE
    └── rolling MAPE を算出して返却
```

未評価レコードの保持期間が短い理由:
- target_time から 20 日経過しても評価できない = 実績データ欠損
- 評価不能なレコードを早めに削除してテーブルを軽量化

## 設定パラメータ

| 環境変数 | デフォルト | 用途 |
|----------|-----------|------|
| `PREDICTION_ACCURACY_WINDOW` | 10 | rolling MAPE 算出に使う直近レコード数 |
| `PREDICTION_ACCURACY_MIN_SAMPLES` | 3 | 評価結果を返す最小レコード数 |
| `PREDICTION_EVAL_TOLERANCE_MINUTES` | 30 | 実績価格検索の時間窓（±分） |
| `PREDICTION_MAPE_EXCELLENT` | 5.0 | MAPE ≤ この値 → confidence = 1.0 |
| `PREDICTION_MAPE_POOR` | 20.0 | MAPE ≥ この値 → confidence = 0.0 |
| `PREDICTION_RECORD_RETENTION_DAYS` | 30 | 評価済みレコードの保持日数 |
| `PREDICTION_UNEVALUATED_RETENTION_DAYS` | 20 | 未評価レコードの保持日数 |

## prediction_confidence による alpha 調整

rolling MAPE を `mape_to_confidence()` で信頼度スコアに変換し、ポートフォリオ最適化の Sharpe/RP ブレンド係数（alpha）を動的に調整する。

### MAPE → confidence 変換

```
confidence = ((MAPE_POOR - mape) / (MAPE_POOR - MAPE_EXCELLENT)).clamp(0.0, 1.0)
```

| MAPE | confidence | 意味 |
|------|-----------|------|
| ≤ 5% | 1.0 | 予測が非常に正確 → Sharpe を信頼 |
| 12.5% | 0.5 | 中程度 |
| ≥ 20% | 0.0 | 予測が不正確 → RP に退避 |

デフォルト値: `DEFAULT_MAPE_EXCELLENT = 5.0`, `DEFAULT_MAPE_POOR = 20.0`
環境変数 `PREDICTION_MAPE_EXCELLENT`, `PREDICTION_MAPE_POOR` で上書き可能

### alpha 計算

```
alpha_vol: ボラティリティベース [0.7, 0.9]
alpha = floor + (alpha_vol - floor) * confidence
floor = PREDICTION_ALPHA_FLOOR (0.5)
```

| 予測精度 | ボラティリティ | alpha | 動作 |
|---------|-------------|-------|------|
| 高 (conf=1.0) | 低 | 0.9 | Sharpe 主導 |
| 高 (conf=1.0) | 高 | 0.7 | RP 補助 |
| データなし | 任意 | 0.7-0.9 | 従来通り（後方互換） |
| 低 (conf=0.0) | 任意 | 0.5 | Sharpe/RP 等配分（最大防御） |

### ソースファイル

| ファイル | 内容 |
|---------|------|
| `backend/src/trade/prediction_accuracy.rs` | `mape_to_confidence()`, 閾値定数 |
| `backend/src/trade/strategy.rs` | MAPE → confidence 変換、PortfolioData への設定 |
| `common/src/algorithm/portfolio.rs` | `PREDICTION_ALPHA_FLOOR`, alpha 計算 |

## Web API

### GET /stats/prediction_mape

直近の rolling MAPE を取得する。

**クエリパラメータ:**

| パラメータ | デフォルト | 説明 |
|-----------|-----------|------|
| `window` | 10 | 直近レコード数 |
| `min_samples` | 3 | 最小サンプル数（未満の場合 `rolling_mape = null`） |

**レスポンス:**

```json
{
  "rolling_mape": 8.45,
  "sample_count": 10,
  "window": 10
}
```
