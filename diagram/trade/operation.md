# 自動トレードシステム 運用ガイド

## 概要

Zaciraciの自動トレードシステムは、NEARプロトコル上のREF Financeを使用して、機械学習予測に基づく自動ポートフォリオ最適化とトレード実行を行います。

> **関連ドキュメント**:
> - トレードルール詳細: `diagram/trade/rules.md`
> - 実装進捗と技術詳細: `diagram/trade/plan.md`

## システム構成

システムは3つのフェーズで構築されており、すべて実装完了済みです：

### ✅ Phase 0: Backend自動トレード基盤
- トレードエントリポイント: `backend/src/trade/stats.rs::start()`
- Cronジョブによる定期実行（毎時0分）
- Portfolio アルゴリズムによる最適化

### ✅ Phase 1: 取引記録システム
- `trade_transactions` テーブルでの取引記録
- TradeRecorderによるバッチ管理とグループ化
- ポートフォリオ価値の時系列追跡

### ✅ Phase 2: 実際のトレード実行
- REF Finance統合による実際のトークンスワップ
- トランザクション成功/失敗の追跡
- ハーベスト機能（利益確定）

## 主要コンポーネント

### 1. トレード実行エンジン (`trade/stats.rs`)
```
毎時0分に実行される自動トレードのメインロジック
- トップボラティリティトークンの選定
- 価格履歴と予測データの取得
- ポートフォリオ最適化
- トレードアクションの実行
```

### 2. スワップ実行 (`trade/swap.rs`)
```
実際のトークン交換を実行
- execute_single_action(): 各トレードアクションの実行
- calculate_total_portfolio_value(): ポートフォリオ価値計算
- TokenRate APIを使用した実際の価格取得
```

### 3. 価格予測 (`trade/predict.rs`)
```
PredictionServiceによる価格予測
- Chronos APIとの統合
- トップトークンの現在価格取得
- 価格履歴データの管理
```

### 4. ハーベスト機能 (`trade/harvest.rs`)
```
利益確定機能
- 初期投資の200%到達時に自動実行
- 余剰分の10%をハーベスト
- wrap.near → NEAR変換と送金
```

### 5. 取引記録 (`trade/recorder.rs`)
```
TradeRecorderによる取引記録
- バッチIDによる関連取引のグループ化
- 成功/失敗の追跡
- yoctoNEAR建て価格の記録
```

## 設定パラメータ

### 環境変数
```bash
# 基本設定
USE_MAINNET=true              # メインネット使用
ROOT_ACCOUNT_ID=              # トレード実行アカウント
ROOT_MNEMONIC=                # ウォレットのニーモニック

# トレード設定
TRADE_INITIAL_INVESTMENT=10   # 初期投資額（NEAR）
TRADE_TOP_TOKENS=10           # 選定するトークン数
TRADE_EVALUATION_DAYS=10      # 評価期間（日）

# ハーベスト設定
HARVEST_ACCOUNT_ID=           # ハーベスト送金先
HARVEST_MIN_AMOUNT=10         # 最小ハーベスト額（NEAR）
```

## Cronジョブスケジュール

```cron
# レート記録（5分毎）
0 */5 * * * * -> record_rates()

# 自動トレード（毎時0分）
0 0 * * * * -> stats::start()
```

## トレードルール

> **詳細**: `diagram/trade/rules.md` を参照

### 基本ルール
1. **評価頻度**: 10日間（`TRADE_EVALUATION_DAYS`）
2. **トレード頻度**: 24時間（毎時0分実行）
3. **トークン選定**: ボラティリティ上位10トークン（`TRADE_TOP_TOKENS`）
4. **ハーベスト条件**: 初期投資の200%超過時に余剰分の10%を収穫
5. **最小ハーベスト額**: 10 NEAR（`HARVEST_MIN_AMOUNT`）
6. **リスク管理**: 最大トレードサイズはポートフォリオ総価値の10%まで

## データフロー

```
1. Cronジョブ起動（毎時0分）
   ↓
2. stats::start() 実行
   ↓
3. トップボラティリティトークン選定
   ↓
4. 価格履歴取得（TokenRate::get_history）
   ↓
5. 価格予測（Chronos API）
   ↓
6. ポートフォリオ最適化（Portfolio Algorithm）
   ↓
7. トレードアクション生成
   ↓
8. 各アクション実行（execute_single_action）
   ↓
9. 取引記録（TradeRecorder）
   ↓
10. ハーベストチェック（check_and_harvest）
```

## モニタリング

### ログ出力
```rust
// slogを使用した詳細ログ
info!(log, "starting trade execution");
warn!(log, "No price data found for token");
error!(log, "failed to execute trading action");
```

### データベース確認
```sql
-- 最新の取引記録確認
SELECT * FROM trade_transactions
ORDER BY created_at DESC
LIMIT 10;

-- バッチ別の取引確認
SELECT batch_id, COUNT(*), SUM(amount_in_yoctonear)
FROM trade_transactions
GROUP BY batch_id
ORDER BY MAX(created_at) DESC;
```

## トラブルシューティング

### 1. トレードが実行されない
- 環境変数の確認
- NEAR残高の確認（初期投資額以上必要）
- Cronジョブの起動確認

### 2. 価格データが取得できない
- TokenRateテーブルのデータ確認
- record_rates()の実行状況確認

### 3. 予測が失敗する
- Chronos APIの稼働状況確認
- 履歴データの十分性確認

### 4. ハーベストが実行されない
- HARVEST_ACCOUNT_IDの設定確認
- ポートフォリオ価値の確認

## 開発・テスト

### テスト実行
```bash
# トレード関連テスト
cargo test --package zaciraci-backend trade

# 特定モジュールのテスト
cargo test --package zaciraci-backend trade::stats::tests
cargo test --package zaciraci-backend trade::predict::tests
cargo test --package zaciraci-backend trade::recorder::tests
```

### ローカル実行
```bash
# 環境変数設定
source run_local/.env

# バックエンドサーバー起動
cargo run --bin zaciraci-backend

# 手動でトレード実行（テスト用）
# backend/src/trade.rsのrun_trade()のCRON_CONFを調整
```

## セキュリティ考慮事項

1. **秘密鍵管理**: ROOT_MNEMONICは安全に管理
2. **最小権限**: トレード実行アカウントには必要最小限の資金のみ
3. **ハーベスト送金先**: 信頼できるアドレスに設定
4. **エラーハンドリング**: 個別取引の失敗が全体を止めないよう設計

## 今後の改善項目

> **実装進捗詳細**: `diagram/trade/plan.md` を参照

### 高優先度
- [ ] リアルタイムモニタリングダッシュボード（Web UI）
- [ ] アラート機能（取引失敗・異常検知）
- [ ] パフォーマンス分析レポート自動生成

### 中優先度
- [ ] trend_followingアルゴリズムの実装
- [ ] 複数アルゴリズムの並列実行（momentum, portfolio, trend_following）
- [ ] リスク管理機能の強化（ドローダウン制限）
- [ ] 設定値の動的調整機能

### 低優先度
- [ ] バックテスト機能の拡充
- [ ] 外部データソースとの統合
- [ ] 機械学習モデルの改良