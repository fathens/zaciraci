# 予測精度トラッキングシステム

## 概要
Chronos AI による価格予測の精度を自動的にトラッキングする。
予測時に prediction_records テーブルへ INSERT し、24時間後の次回実行時に実績価格と比較して精度指標（MAPE）を算出する。

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
    rolling MAPE (ログ出力)
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
```

## 設定パラメータ

| 環境変数 | デフォルト | 用途 |
|----------|-----------|------|
| `PREDICTION_ACCURACY_WINDOW` | 10 | rolling MAPE 算出に使う直近レコード数 |
| `PREDICTION_ACCURACY_MIN_SAMPLES` | 3 | 評価結果を返す最小レコード数 |
| `PREDICTION_EVAL_TOLERANCE_MINUTES` | 30 | 実績価格検索の時間窓（±分） |
