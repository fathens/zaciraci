# 自動トレードの開発プラン

## 📚 関連ドキュメント

- **変更履歴**: [CHANGELOG.md](./CHANGELOG.md) - 完了済み項目の詳細
- **未実装タスク**: [TODO.md](./TODO.md) - 優先順位付きタスクリスト
- **セットアップガイド**: [/TRADING_SETUP.md](../../TRADING_SETUP.md) - ユーザー向けセットアップ手順
- **トレードルール**: [rules.md](./rules.md) - 自動トレードのルール定義
- **取引記録**: [records.md](./records.md) - データベース設計

## 🎯 現在の状況

### ✅ Phase 0-2: 完了（詳細はCHANGELOG.md参照）

自動トレードシステムは**完全実装済み**で、以下の機能が稼働中：

- **Phase 0**: Backend 自動トレード基盤
- **Phase 1**: 取引記録と架空トレード
- **Phase 2**: 実際のトレード実行

### 🔥 現在の優先タスク（2025-10-10更新）

詳細は [TODO.md](./TODO.md) を参照。

#### ✅ 最近完了した項目
- クライアント側ポーリング実装（wait_until=NONE）
- Storage Deposit 一括セットアップ
- マルチエンドポイントRPC 完全実装（Phase 1, 2a, 2b）
- リトライロジックのバグ修正

#### ⏳ 優先度: 高
1. record_rates 間隔調整（5分→15分）
2. BigDecimal 変換の網羅チェック

## 🚀 次のステップ

1. **運用監視**: 次回cron実行でStorage Deposit実装の効果確認
2. **RPC負荷軽減**: record_rates間隔を15分に調整
3. **コード品質**: BigDecimal変換箇所の網羅的チェック
4. **機能拡張**: 追加の取引戦略実装（Momentum、TrendFollowing）

## 📖 設定パラメータ

詳細は [TRADING_SETUP.md](../../TRADING_SETUP.md) を参照してください。主なパラメータ：

- `TRADE_INITIAL_INVESTMENT`: 初期投資額
- `TRADE_TOP_TOKENS`: 選定トークン数（デフォルト: 10）
- `TRADE_EVALUATION_DAYS`: 評価頻度（デフォルト: 10日）
- `HARVEST_MIN_AMOUNT`: 最小ハーベスト額（デフォルト: 10 NEAR）
- `HARVEST_ACCOUNT_ID`: ハーベスト送金先

## 🏗️ アーキテクチャ

### コア実装
- `backend/src/trade/stats.rs`: トレードエントリポイント
- `backend/src/trade/predict.rs`: 価格予測サービス
- `backend/src/trade/harvest.rs`: ハーベスト機能
- `backend/src/trade/recorder.rs`: 取引記録

### 統合
- `backend/src/ref_finance/`: REF Finance統合
- `backend/src/jsonrpc/`: NEAR RPC統合
- `backend/src/persistence/`: データベース永続化

### アルゴリズム（common/src/algorithm/）
- `portfolio.rs`: Portfolio最適化アルゴリズム
- `momentum.rs`: Momentum戦略（実装済み、未使用）
- `trend_following.rs`: TrendFollowing戦略（実装済み、未使用）

## 📝 開発ガイドライン

### トレード実行フロー
1. **資金準備**: NEAR → wrap.near 変換
2. **トークン選定**: ボラティリティTop10選択
3. **ポートフォリオ最適化**: Portfolioアルゴリズムで最適配分計算
4. **取引実行**: TradingActionに基づくswap実行
5. **ハーベスト**: 200%利益時に10%を収穫

### 技術スタック
- **言語**: Rust
- **データベース**: PostgreSQL + Diesel
- **API**: Chronos（価格予測）、NEAR RPC
- **取引所**: REF Finance
- **スケジューリング**: cron

### コーディング規約
- BigDecimal を使用した高精度計算
- slog による構造化ログ
- 非同期処理（tokio）
- エラーハンドリング（anyhow）
