# 自動トレードのルール

## 基本設定

### アルゴリズム
- **使用アルゴリズム**: Portfolio
- **実装場所**: `zaciraci_common::algorithm::portfolio`

### 評価期間
- **設定項目**: `TRADE_EVALUATION_DAYS`
- **デフォルト値**: 10日間
- **説明**: 価格履歴を取得して予測を行う際の過去データの期間

### トレード頻度
- **設定項目**: `TRADE_FREQUENCY_HOURS`
- **デフォルト値**: 24時間（1日1回）
- **説明**: 定期的なトレード実行の間隔
- **現在の実装**: backend/src/trade.rs で cron 設定（毎時0分実行）

