# CLI Commands Overview

## 概要
volatility tokens の分析と予測を行うためのCLIツール

## コマンド構成

### 1. top コマンド
指定された期間の volatility tokens データを取得

- 10日程度のデータを効率的に取得可能
- 取得したデータはJSON形式で保存
- 時系列データの可視化にも対応

### 2. predict コマンド
時系列データを用いて将来の予測を実行

- Chronos API を使用した機械学習による予測
- AutoGluon モデルによる高精度な時系列予測
- 非同期処理による長時間の予測タスクに対応

### 3. verify コマンド（未実装）
予測結果の精度を検証

- 実際のデータと予測データの比較
- 精度評価指標（MAE, RMSE, MAPE）の計算
- 検証レポートの生成

## 基本的な使用手順

```bash
# 1. 過去10日程度のデータを取得
cargo run -- top --start 2025-06-25 --end 2025-07-04 --output cli_test/.work

# 2. 取得したデータを使用して予測を実行
cargo run -- predict cli_test/.work/tokens/[token].json --output cli_test/.work/predictions

# 3. 予測結果の検証（未実装）
cargo run -- verify cli_test/.work/predictions/[token]/prediction.json
```

## データ要件
- 予測には最低180データポイントが必要（AutoGluon要件）
- 10日程度のデータで十分な精度の予測が可能
- 2時間間隔のデータポイントで高精度な予測を実現