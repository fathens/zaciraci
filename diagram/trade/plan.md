# 自動トレードの開発プラン

段階的に作っていく。

cli_tokens で Chronos API を使う実績のあるコードが common にあるのでそれを使う。

## ✅ Phase 0: Backend 自動トレード基盤 (完了)

**実装済み**: Portfolio アルゴリズムを使用した自動トレードシステムの基盤を backend に実装。

### 実装した機能

- **トレードエントリポイント**: `backend/src/trade/stats.rs` の `start()` 関数
- **資金準備**: NEAR → wrap.near 変換処理
- **トークン選定**: top volatility トークンの選択（PredictionService使用）
- **ポートフォリオ最適化**: 既存の Portfolio アルゴリズムを活用
- **価格予測**: Chronos API を使用した価格予測（PredictionService経由）
- **ボラティリティ計算**: BigDecimal を使用した高精度計算（Newton法平方根）
- **Cron統合**: `trade.rs` で毎時0分に自動実行

### ルール準拠

`rules.md` で定義されたトレードルールに完全対応:
- 評価頻度: 10日間（`TRADE_EVALUATION_DAYS`）
- トレード頻度: 24時間（毎時0分実行）
- トークン選定: top 10 volatility（`TRADE_TOP_TOKENS`）
- ハーベスト: 200%超過の10%を収穫

### 設定パラメータ

- `TRADE_INITIAL_INVESTMENT`: 初期投資額
- `TRADE_TOP_TOKENS`: 選定トークン数（デフォルト: 10）
- `TRADE_EVALUATION_DAYS`: 評価頻度（デフォルト: 10日）
- `HARVEST_MIN_AMOUNT`: 最小ハーベスト額（デフォルト: 10 NEAR）
- `HARVEST_ACCOUNT_ID`: ハーベスト送金先

### 技術的実装詳細

- **データ精度**: f64 からの脱却を目指し BigDecimal 中心の実装
- **API統合**: 既存 PredictionService の活用でコード重複を排除
- **エラーハンドリング**: プレースホルダー関数を排除し適切なエラー処理を実装

## Phase 1

トレードは実際には行わず、仮にトレードしたとしてどういう実績になるかを調査する。

### 必要なコード

#### 価格情報と予測

* 指定期間の top の取得
* 指定期間の history の取得
* 指定期間の情報を元に指定した日時の価格の予測

#### 予測を元にトレード

##### 決定アルゴリズム

* 保有トークンと他のトークンの価格予測を元にトレード内容を決定
* トレードの手数料を計算

**決定済み**: Portfolioアルゴリズムを使用

##### 架空トレード

* 実際にはトレードせずに DB に記録
* DB にあるトークンの保有量と価格を掛け合わせて資産評価

TODO: DB テーブルの設計

## Phase 2

実際にトレードする。

### 必要なコード

#### 価格情報と予測

Phase 1 のを使う

#### 予測を元にトレード

Phase 1 の決定アルゴリズムを使う

##### 実際のトレード

* Tx の作成と送信（既にあるはずなのでそれを使う）
* Tx の成否の確認
* DB に記録（Phase 1 のを使う）
* DB から資産評価（Phase 1 のを使う）

## ✅ Phase 0 完了済み項目 (2025-09-17 更新)

### 完了した重要な修正
1. **エラーハンドリング強化** ✅
   - 危険な `unwrap()` 呼び出しを適切なエラーハンドリングに変更
   - `get_top_quote_token()`, `get_base_tokens()` での適切なエラー処理
   - テスト修正と全テスト成功 (trade::stats: 13件, trade::predict: 7件)

2. **数値計算の修正** ✅
   - `calculate_liquidity_score()` の対数変換バグ修正
   - Newton法平方根計算でのBigDecimal精度確保
   - CI/CDチェック通過 (cargo clippy, cargo fmt)

## 🚧 次の優先実装項目

### 🔧 高優先度 (High Priority) - 即座に実装

1. **実際のトークン交換実行**:
   - `stats.rs:429-435`: `execute_single_action()` でのTODO実装
   - 既存 arbitrage.rs の swap 実装を活用してswap処理実装
   - パス検索: token → wrap.near → target の経路最適化
   - トランザクション成否の確認とエラーハンドリング
   - **現在**: `warn!("swap execution not yet implemented")` 状態

2. **f64 使用の完全排除** (残存部分):
   - Portfolio アルゴリズム用の f64 → BigDecimal 変換
   - `predict.rs:44,179,414`: PredictedPrice の price, confidence フィールド
   - 外部ライブラリ制約への対応（zaciraci_common の構造体）

### 🛠 中優先度 (Medium Priority)

1. **ハーベスト機能の実装**:
   - `stats.rs:320-326`: 200%利益時の自動利益確定機能
   - wrap.near → NEAR 変換と送金処理
   - ハーベスト条件判定の実装

2. **ハードコード値の実装**:
   - `stats.rs:280-281`: liquidity_score, market_cap の動的取得
   - 実際の流動性スコア計算アルゴリズム
   - 市場規模データの取得方法

### 🔄 改善項目 (Low Priority)

1. **設定管理の強化**:
   - 環境変数による設定の外部化
   - 設定値のバリデーション
   - デフォルト値の適切な設定

2. **ログ・モニタリング機能**:
   - 取引実績の詳細記録
   - ポートフォリオパフォーマンスの追跡
   - アラート機能の実装

## 🎯 推奨する次のアクション

### 即座に取り組むべきタスク

**最優先: 実際のトークン交換実行の実装**

1. **arbitrage.rs の分析** (Phase 1-A):
   - 既存の swap 実装パターンを理解
   - REF Finance API 呼び出し方法の把握
   - パス検索アルゴリズムの理解

2. **trade モジュールへの統合** (Phase 1-B):
   - `execute_single_action()` 関数の実装
   - arbitrage.rs のコードを trade 用途に適応
   - token → wrap.near → target の経路実装

3. **テストと検証** (Phase 1-C):
   - swap 実行のテストケース作成
   - トランザクション成否の確認機能
   - エラーハンドリングの強化

### 実装順序
```
Phase 1-A → Phase 1-B → Phase 1-C → 高優先度項目2 (f64排除)
```

これにより架空トレードから実際のトレードへの移行が完了し、Phase 1 (実際のトレード実行) に到達できます。

## 次のステップ

### 短期（Phase 0 改善 → Phase 1）

1. **実際のトレード機能実装** ⭐ 最優先:
   - REF Finance 統合
   - トランザクション処理
   - arbitrage.rs との統合

2. **データ精度問題の解決**:
   - BigDecimal の完全採用
   - 外部ライブラリとの整合性確保

3. **DB テーブル設計と実装**:
   - 架空トレード記録テーブル
   - ポートフォリオ評価履歴テーブル

### 中期（Phase 1 → Phase 2）

1. **実際のトレード実行**:
   - 既存の swap 実装を活用
   - トランザクション成否の確認
   - エラーハンドリングとリトライ機能

2. **資金管理**:
   - NEAR/wrap.near 変換の実装
   - ハーベスト時の実際の送金処理

3. **モニタリング**:
   - トレード結果の追跡
   - パフォーマンス分析機能
