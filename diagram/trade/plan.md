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

## ✅ Phase 1: 取引記録と架空トレード (完了)

**実装済み**: トレード記録システムを完全実装。実際のトレード実行と記録機能が統合され、取引実績の追跡が可能。

### 必要なコード

#### 価格情報と予測

* ✅ 指定期間の top の取得（完了）
* ✅ 指定期間の history の取得（完了）
* ✅ 指定期間の情報を元に指定した日時の価格の予測（完了）

#### 予測を元にトレード

##### 決定アルゴリズム

* ✅ 保有トークンと他のトークンの価格予測を元にトレード内容を決定（完了）
* ✅ トレードの手数料を計算（完了）

**決定済み**: Portfolioアルゴリズムを使用

##### 架空トレード

* 実際にはトレードせずに DB に記録
* DB にあるトークンの保有量と価格を掛け合わせて資産評価

**実装予定**: trade_transactions テーブル（records.md 参照）

### ✅ trade_transactions テーブル実装状況 (完了)

#### ✅ Phase 1: 基本記録機能 (完了)
- [x] データベーススキーマ設計（records.md 完了）
- [x] Diesel migration ファイル作成 ✅ **2025-09-19 完了**
- [x] Rust struct 定義 (`TradeTransaction`) ✅ **2025-09-19 完了**
- [x] 基本的な記録機能実装 ✅ **2025-09-19 完了**
- [x] データベース接続とCRUD操作 ✅ **2025-09-19 完了**

#### ✅ Phase 2: 取引連携 (完了)
- [x] 実際の取引実行機能（execute_single_action 完了）
- [x] 取引成功時の自動記録 ✅ **2025-09-19 完了**
- [x] バッチID生成と管理 ✅ **2025-09-19 完了**
- [x] トランザクションハッシュの取得と保存 ✅ **2025-09-19 完了**
- [x] エラーハンドリング ✅ **2025-09-19 完了**

#### ✅ Phase 3: 分析機能 (完了)
- [x] ポートフォリオ価値の集計 ✅ **2025-09-19 完了**
- [x] 時系列データの取得 ✅ **2025-09-19 完了**
- [x] パフォーマンス分析 ✅ **2025-09-19 完了**

### 🏆 取引記録システム完成サマリー (2025-09-19)

**実装された主要機能:**
- ✅ **TradeTransaction構造体**: 同期・非同期CRUD操作完全対応
- ✅ **TradeRecorder**: バッチ管理と取引グループ化機能
- ✅ **自動記録統合**: execute_single_action での成功時自動記録
- ✅ **データベース設計**: 適切なインデックス付きtrade_transactionsテーブル
- ✅ **connection_poolカプセル化**: persistenceモジュール内完全封じ込め
- ✅ **テスト網羅**: 全38のトレード関連テスト成功
- ✅ **Migration実行**: PostgreSQL環境での動作確認完了

**技術的実装詳細:**
- **yoctoNEAR建て価格記録**: BigDecimal使用による高精度計算
- **バッチID管理**: UUID使用による取引グループ化
- **非同期データベース操作**: deadpool-dieselによる効率的な接続管理
- **適切なアーキテクチャ**: token_rateパターンに準拠したカプセル化

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

## ✅ Phase 0 完了済み項目 (2025-09-18 更新)

### 完了した重要な修正
1. **エラーハンドリング強化** ✅
   - 危険な `unwrap()` 呼び出しを適切なエラーハンドリングに変更
   - `get_top_quote_token()`, `get_base_tokens()` での適切なエラー処理
   - テスト修正と全テスト成功 (trade::stats: 13件, trade::predict: 7件)

2. **数値計算の修正** ✅
   - `calculate_liquidity_score()` の対数変換バグ修正
   - Newton法平方根計算でのBigDecimal精度確保
   - CI/CDチェック通過 (cargo clippy, cargo fmt)

3. **実際のトークン交換実行機能** ✅ **新規完了 (2025-09-18)**
   - `execute_single_action()` 関数の完全実装
   - arbitrage.rs のswapフレームワークを活用した実装
   - 全TradingActionパターンの対応:
     - `Hold`: ポジション保持
     - `Sell`: token → wrap.near → target (2段階swap)
     - `Switch`: from → to (直接swap)
     - `Rebalance`: 目標ウェイトに基づく複数swap実行
     - `AddPosition`: wrap.near → token swap
     - `ReducePosition`: token → wrap.near swap
   - `execute_direct_swap()` ヘルパー関数の実装
   - プールパス検索、ストレージデポジット、トランザクション待機まで含む完全な実装
   - ref_financeインフラとの完全統合

4. **f64からBigDecimalへの完全移行** ✅ **新規完了 (2025-09-18)**
   - 全財務計算でBigDecimal使用
   - 予測信頼度値のBigDecimal対応
   - 型整合性問題の解決
   - 全235テストの成功確認

## 🏆 ハーベスト機能完成サマリー (2025-09-19)

**実装された主要機能:**
- ✅ **ポートフォリオ価値計算**: trade_transactionsテーブルからの自動集計
- ✅ **利益判定ロジック**: 初期投資額の300%到達時の自動検出
- ✅ **利益確定アルゴリズム**: 余剰分の10%を自動ハーベスト
- ✅ **設定可能なパラメータ**: HARVEST_ACCOUNT_ID、HARVEST_MIN_AMOUNT
- ✅ **送金フレームワーク**: 既存ref_financeインフラとの統合準備
- ✅ **エラーハンドリング**: 堅牢なBigDecimal計算とバリデーション

**技術的実装詳細:**
- **BigDecimal精度**: 高精度yoctoNEAR計算による利益計算
- **データベース統合**: TradeTransaction::get_latest_batch_id_async使用
- **設定管理**: 環境変数による柔軟なハーベスト設定
- **ログ記録**: 詳細なハーベスト実行ログ

**完了した統合ステップ: ✅ (2025-09-19)**
- ✅ ref_finance::deposit::wnear::unwrapとの実際の統合
- ✅ wrap.near → NEAR変換の完全実装
- ✅ ハーベスト実行のTradeTransaction記録

**統合実装詳細:**
- `execute_harvest_transfer`: 完全なハーベスト実行フロー
- `deposit::withdraw`: ref_financeからのwrap.near引き出し
- `deposit::wnear::unwrap`: wrap.nearからNEARへの変換
- `transfer_native_token`: 指定アカウントへのNEAR送金
- `TradeRecorder`: ハーベスト取引の自動記録

**✅ 改善実装 (2025-09-19):**
- **Lazy初期化**: `HARVEST_ACCOUNT`と`HARVEST_MIN_AMOUNT`のstatic変数化
- **時間管理**: `LAST_HARVEST_TIME`とインターバル制御の実装
- **設定管理**: balances.rsと一貫性のある設定パターン
- **パフォーマンス**: 実行時設定読み込みを排除した効率的な実装
- **エラーハンドリング**: 設定不備時の適切なデフォルト値対応

**✅ Swap機能改善 (2025-09-19):**
- **arbitrage.rsパターン採用**: `execute_swap_with_recording`関数の実装
- **並列処理**: `futures_util::future::join_all`による複数swap並列実行
- **型安全性**: ジェネリック型による`MicroNear`と`Balance`の両対応
- **エラー処理**: 個別swap失敗時も他のswapを継続実行
- **成功率追跡**: swap完了時の成功/失敗数の詳細ログ記録
- **自動記録**: 各swap成功時の`TradeTransaction`への自動記録

## 🚧 次の優先実装項目

### 🔧 高優先度 (High Priority) - 即座に実装

1. ~~**実際のトークン交換実行**~~ ✅ **完了 (2025-09-18)**:
   - ~~`stats.rs:429-435`: `execute_single_action()` でのTODO実装~~ ✅
   - ~~既存 arbitrage.rs の swap 実装を活用してswap処理実装~~ ✅
   - ~~パス検索: token → wrap.near → target の経路最適化~~ ✅
   - ~~トランザクション成否の確認とエラーハンドリング~~ ✅
   - ~~**現在**: `warn!("swap execution not yet implemented")` 状態~~ ✅

2. ~~**f64 使用の完全排除**~~ ✅ **完了 (2025-09-18)**:
   - ~~Portfolio アルゴリズム用の f64 → BigDecimal 変換~~ ✅
   - ~~`predict.rs:44,179,414`: PredictedPrice の price, confidence フィールド~~ ✅
   - ~~外部ライブラリ制約への対応（zaciraci_common の構造体）~~ ✅

### ✅ 高優先度 (High Priority) - 完了済み

1. ~~**取引記録システムの実装**~~ ✅ **完了 (2025-09-19)** (records.md):
   - ✅ trade_transactions テーブルの作成
   - ✅ TradeTransaction struct の定義
   - ✅ 取引成功時の自動記録機能
   - ✅ バッチIDによる取引グループ管理
   - ✅ ポートフォリオ価値の時系列追跡

### ✅ 最高優先度 (Next Priority) - 完了済み (2025-09-19)

1. ~~**ハーベスト機能の実装**~~ ✅ **完了 (2025-09-19)**:
   - ✅ `stats.rs:695-773`: `check_and_harvest()` 関数の実装
   - ✅ 200%利益時の自動利益確定機能（3倍で10%ハーベスト）
   - ✅ wrap.near → NEAR 変換と送金処理のフレームワーク
   - ✅ ハーベスト条件判定の実装（最小額設定可能）
   - ✅ trade_transactions テーブルとの連携

### 🛠 中優先度 (Medium Priority)

1. **ハードコード値の実装**:
   - `stats.rs:318-319`: liquidity_score, market_cap の動的取得
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

### ✅ Phase 0 完了: 実際のトレード実行機能

**~~最優先: 実際のトークン交換実行の実装~~** ✅ **完了 (2025-09-18)**

1. ~~**arbitrage.rs の分析**~~ ✅ **完了**:
   - ~~既存の swap 実装パターンを理解~~ ✅
   - ~~REF Finance API 呼び出し方法の把握~~ ✅
   - ~~パス検索アルゴリズムの理解~~ ✅

2. ~~**trade モジュールへの統合**~~ ✅ **完了**:
   - ~~`execute_single_action()` 関数の実装~~ ✅
   - ~~arbitrage.rs のコードを trade 用途に適応~~ ✅
   - ~~token → wrap.near → target の経路実装~~ ✅

3. ~~**テストと検証**~~ ✅ **完了**:
   - ~~swap 実行のテストケース作成~~ ✅
   - ~~トランザクション成否の確認機能~~ ✅
   - ~~エラーハンドリングの強化~~ ✅

### 次の実装ターゲット: 取引記録システムとハーベスト機能

**新しい最優先: 取引記録システム (records.md の実装)**

1. **trade_transactions テーブルの実装** (最優先):
   - Diesel migration ファイル作成
   - Rust struct 定義 (`TradeTransaction`)
   - 基本的な記録機能実装
   - 成功した取引の自動記録機能

2. **取引記録の統合** (優先度高):
   - `execute_single_action()` での取引成功時の記録
   - バッチIDによる関連取引のグループ化
   - トランザクションハッシュの保存
   - yoctoNEAR建て価格の記録

3. **分析機能の実装** (優先度中):
   - ポートフォリオ価値の時系列追跡
   - バッチ別の取引詳細表示
   - パフォーマンス分析クエリ

**次に優先: ハーベスト機能の実装**

1. **ポートフォリオ価値計算** (ハーベスト Phase A):
   - 現在の保有トークン残高取得
   - 各トークンの現在価格取得
   - 総ポートフォリオ価値の計算
   - **trade_transactions テーブルからの履歴データ活用**

2. **ハーベスト条件判定** (ハーベスト Phase B):
   - 初期投資額との比較
   - 200%超過時の10%利益確定ロジック
   - ハーベスト実行タイミングの判定
   - **取引履歴データに基づく利益計算**

3. **実際のハーベスト実行** (ハーベスト Phase C):
   - 利益分のトークン → wrap.near swap
   - wrap.near → NEAR 変換
   - 指定アドレスへの送金処理
   - **ハーベスト取引の記録**

これによりPhase 0が完全に完了し、取引実績の追跡が可能となり、Phase 1 (架空トレード記録) への準備が整います。

## 次のステップ

### 短期（Phase 0 完成 → Phase 1）

1. ~~**実際のトレード機能実装**~~ ✅ **完了 (2025-09-18)**:
   - ~~REF Finance 統合~~ ✅
   - ~~トランザクション処理~~ ✅
   - ~~arbitrage.rs との統合~~ ✅

2. ~~**データ精度問題の解決**~~ ✅ **完了 (2025-09-18)**:
   - ~~BigDecimal の完全採用~~ ✅
   - ~~外部ライブラリとの整合性確保~~ ✅

3. **ハーベスト機能実装** ⭐ **次の最優先**:
   - ポートフォリオ価値計算
   - 利益確定条件判定
   - 実際のハーベスト実行

4. **DB テーブル設計と実装**:
   - trade_transactions テーブル（実取引記録）
   - 架空トレード記録テーブル
   - ポートフォリオ評価履歴テーブル

### 中期（Phase 1 → Phase 2）

1. ~~**実際のトレード実行**~~ ✅ **完了 (2025-09-18)**:
   - ~~既存の swap 実装を活用~~ ✅
   - ~~トランザクション成否の確認~~ ✅
   - ~~エラーハンドリングとリトライ機能~~ ✅

2. **資金管理** (部分完了):
   - ~~NEAR/wrap.near 変換の実装~~ ✅
   - ハーベスト時の実際の送金処理 (TODO)

3. **モニタリング**:
   - トレード結果の追跡
   - パフォーマンス分析機能

## 🏆 Phase 0 完了サマリー (2025-09-18)

**Phase 0: Backend 自動トレード基盤** が完全に完了しました！

### 達成した主要機能
✅ **トレードエントリポイント**: `trade::stats::start()` 関数
✅ **資金準備**: NEAR → wrap.near 変換処理
✅ **トークン選定**: top volatility トークンの選択
✅ **ポートフォリオ最適化**: Portfolio アルゴリズムの完全統合
✅ **価格予測**: Chronos API を使用した価格予測
✅ **実際のトレード実行**: 全TradingActionパターンの完全実装
✅ **数値精度**: f64からBigDecimalへの完全移行
✅ **エラーハンドリング**: 堅牢なエラー処理の実装

### 次の目標: ハーベスト機能
Phase 0の最後のピースとして、ハーベスト機能の実装が残されています。これが完了すれば、Phase 1 (架空トレード記録) への移行準備が整います。
