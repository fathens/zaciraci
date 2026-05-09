# Price-Impact-Aware Portfolio Optimization 改善計画

作成日: 2026-05-05
最終改訂: 2026-05-05（チームレビュー結果を反映）
対象シミュレーション: `simulation_2026-04-06_to_2026-04-16_v3.json`
背景成果: cost-aware mode の構造は既存。実環境/シミュレーションで**機能していない**箇所の特定と修正。

---

## 1. 背景

v3 シミュレーション（2026-04-06〜04-16, initial=100 NEAR, cost-aware ON）で
`Day 1` に **-41.27 NEAR (-41%)** の即時損失が発生した。期末には -56% 累計損失。

損失の 99% は流動性極小トークン 3 銘柄に集中:

| トークン | 投入 (NEAR) | 期末 realized P&L (NEAR) |
|---|---|---|
| `mineminemine.tkn.primitives.near` | 16.86 | **-27.15** |
| `yupland.tkn.near` | ~14 | **-14.94** |
| `mpdao-token.near` | 14.36 | **-13.07** |

`100 NEAR / 6 銘柄 ≒ 16 NEAR` の等分割が AMM 上の極端な price impact を引き起こした。
**ただしレビューの結果、損失の主因は単純な price impact ではなく、cost 推定モデルの構造的欠陥（片道のみ・target 値ベース）であることが判明**（§3 参照）。

## 2. 既存実装の調査結果

「価格インパクトを最適化に組み込む」設計は **既に存在する**。

### 2.1 既存コンポーネント

- `crates/trade/src/cost.rs::estimate_trade_cost`
  - `compute_variable_ratio` が `(input_NEAR - output_via_path_NEAR) / input_NEAR` で
    AMM fee + price impact を一括算出
  - `EXPECTED_SLIPPAGE_DEDUCTION = 0.005` を定数加算
- `crates/trade/src/portfolio_cost.rs::compute_cost_deductions`
  - `assumed_in[i] = total_value × weights[i]` で銘柄ごとの想定取引額を算出
  - 各銘柄について `estimate_trade_cost` を呼び `cost_deduction` を Markowitz に渡す
- `crates/trade/src/portfolio_cost.rs::run_cost_aware_optimization`
  - fixed-point iteration（max_iter, damping, convergence_tolerance=1e-3）
- `crates/trade/src/strategy.rs::execute_portfolio_strategy`（line 903 付近）
  - `cfg.trade_cost_aware_return_enabled()` のとき上記を呼ぶ
- `crates/trade/src/strategy.rs::apply_liquidity_filter_and_select`
  - `TRADE_MIN_POOL_LIQUIDITY` 未満のプールを候補から除外
- `crates/simulate/src/engine.rs::apply_config`
  - simulate 既定で `cost_aware_return = true`, `cost_iterations_max = 3`
- `crates/persistence/src/token_rate.rs::to_spot_rate_with_fallback` (line 534-549)
  - per-hop `correction = Π_i (1 + Δx_i / x_i)` で AMM impact 補正を適用
  - `Δx = trade_min_pool_liquidity / 10` ≒ 10 NEAR をサンプル

### 2.2 結論

> **「選択時 C」は新規実装不要。既存実装が機能していない原因を特定し直す**。
> ただし、レビューの結果として **既存の cost 推定モデル自体に構造的欠陥がある**ことが判明。
> 詳細は §3 を参照。

---

## 3. 機能していない原因（レビュー後・確定版）

### CRITICAL #1. 片道（BUY）コストしかモデル化していない [損失の主因]

`crates/trade/src/portfolio_cost.rs::compute_cost_deductions` は
`path = wnear → token` の **BUY 側のみ** `estimate_trade_cost` に渡す。
SELL 側 (`token → wnear`) の symmetric impact は未考慮。

v3 実データ確認:
- mineminemine に 6 回 BUY を蓄積 (16.86 + 3.58 + 6.27 + 0.76 + 0.22 + 0.20 = 28 NEAR)
- 最終 liquidation で `realized_pnl = -27.15 NEAR`
- 片道 14% × 6 回 ≒ 85% 累積、+ 自分の BUY が pool 価格を歪めて自分の SELL を不利化
- **-160% は「片道インパクト × N 回 + 価格 self-impact」で物理的に説明可能**

### CRITICAL #2. `assumed_in[i] = total_value × weights[i]` は target 値、差分取引額ではない

`crates/trade/src/portfolio_cost.rs:164` で全ターゲット値を `assumed_in` にしている。
初期購入（holdings=0）は OK だが、リバランスでは実取引は `|target - current|`。
結果: 安定保有時にコスト推定が過大化 → 過剰除外 / 過小投資 / Hold 倒れの誘発。

### CRITICAL #3. プール状態の時刻不整合

`crates/trade/src/portfolio_cost.rs:94`:
```rust
let pools = persistence::pool_info::read_from_db(None).await?;
```

`crates/trade/src/strategy.rs:390`（liquidity filter 用）:
```rust
let pools = persistence::pool_info::read_from_db(None).await?;
```

両方とも `None` を渡しており、**シミュレーション当日 (sim_day) ではなく
テスト DB の最新スナップショット**のプール状態を使っている。

一方:
- `crates/simulate/src/mock_client.rs:147`: `read_from_db(Some(sim_day.naive_utc()))`
  - **swap 実行時** は正しく sim_day のプール状態を使用

→ **コスト推定と実行で時刻が乖離**。

### WARNING. 流動性閾値の緩さ + ポジション/プール比率制約の不在

`TRADE_MIN_POOL_LIQUIDITY` の既定値 100 NEAR では、メモコイン系プール
(`*.meme-cooking.near`, `mineminemine.tkn.primitives.near`) を排除しきれない。
さらに、**注文額がプール TVL の何 % か**という相対サイズ制約が存在しない。
100 NEAR プールに 16 NEAR は 16% で、典型的に大幅スリッページ。

### 撤回された仮説（旧 H2: spot_rate の循環依存）

旧計画では「`spot_rate` が同じ薄いプール由来なら price impact ≒ 0 と誤計算される」と
仮定していたが、`persistence/src/token_rate.rs::to_spot_rate_with_fallback`
(line 534-549) で per-hop `correction = Π_i (1 + Δx_i / x_i)` の AMM 補正が
**既に正しく実装されている**ことを確認。

数値検証: 単一プール TVL=100 NEAR, 取引 A=16 NEAR, fee f=0.003 →
`compute_variable_ratio` の `amm_loss = (1-f)·A/(x+(1-f)·A) ≈ 13.76%` を
**正しく検出する**。

→ 旧 P3 (`amm_loss < 1 BPS` fail-safe) は **撤回**。
fail-safe の代わりに `(deduction[i] / weight[i])` の感度を反復ループで観測する方針へ転換（§5 ステップ 8 参照）。

### 補足. EXPECTED_SLIPPAGE_DEDUCTION の役割

`EXPECTED_SLIPPAGE_DEDUCTION = 0.005` (50 BPS) は「予期しない実行時誤差」用の固定 buffer。
サイズ依存の price impact は `compute_variable_ratio` が AMM 公式から計算する。
本計画では維持（変更不要）。

---

## 4. 修正項目（優先度順）

### P1. 対称コスト推定（CRITICAL #1 対応）— 必須・損失主因

**変更**:
- `PortfolioCostInputs` を `paths: BTreeMap` + `rates: BTreeMap` の並行 BTreeMap から、
  `bundles: BTreeMap<TokenOutAccount, TokenSwapBundle>` に集約
- `TokenSwapBundle { buy_path, sell_path, rate }` を新規定義
- `compute_cost_deductions` で BUY と SELL 両方の `estimate_trade_cost` を合算
- `compute_variable_ratio_total = buy_variable + sell_variable`

**期待効果**: cost 推定が往復インパクトを正しく反映 → メモコイン系の真のコストが Markowitz に伝わる。

**リスク**:
- `graph.update_graph(start)` は一方向トラバース（`graph.rs:122`）。SELL path の独立探索には
  各 token を start にした update_graph 呼び出しが必要 → O(N tokens × graph traversal)
- Commit 6 実装時に `Arc<TokenGraph>` cache 共有 or `update_graph` 双方向対応の必要性を検証

### P2. `assumed_in` を差分計算に修正（CRITICAL #2 対応）— 必須

**変更**:
- `compute_cost_deductions` のシグネチャを `(weights, inputs, wallet_info: &WalletInfo)` の 3 引数に縮小
  - `WalletInfo.total_value` + `WalletInfo.holdings` から両方取得
- `assumed_in` を `trade_delta_yocto` にリネーム
- 計算式を `|target_value - current_value_at_spot|` に変更
- 符号付き差分は `BigDecimal` 直接演算 + `.abs()`（`YoctoValue` は unsigned）

**期待効果**: 安定保有時のコスト推定過大評価が解消 → 適切な weight に収束。

### P3. プール状態の時刻整合（CRITICAL #3 対応）— 必須・根治

**変更**:
- `collect_cost_inputs` のシグネチャに `as_of: DateTime<Utc>`（**非 Option**）を追加
- `read_from_db(None)` → `read_from_db(Some(as_of.naive_utc()))`
- `select_volatility_tokens_inner` は既に `end_date: DateTime<Utc>` 引数を持つため、
  内部の `read_from_db(None)` を `read_from_db(Some(end_date.naive_utc()))` に変更（1 行修正）
- 呼び出し側 (`execute_portfolio_strategy`) で `params.end_date` 経路で thread
  - production: `Utc::now()`、simulate: `sim_day` を渡す
  - `apply_config` 経由は禁忌（process-wide config の濫用を避ける）

**期待効果**: シミュレーション時のコスト推定が当日のプール状態を反映 →
実行とコスト推定の乖離が解消。

**設計判断**: `Option<DateTime<Utc>>` は採用しない。`run_prediction_cycle` の既存 convention
（`as_of: chrono::DateTime<chrono::Utc>` 非 Option）に統一。

### P4. ポジション/プール比率制約（WARNING 対応）— 短期で効果大

**変更**:
- 設定キー `TRADE_MAX_POSITION_VS_POOL_RATIO`（型 `f64`）を新設
  - 既定値: **0.02 (2%)**
  - clamp: `.clamp(0.001, 0.5)` + NaN handler（`clamp_portfolio_pred_err_diagonal_k` パターン踏襲）
- `portfolio_cost.rs::compute_cost_deductions` 内で
  `trade_delta_yocto[i] > path_min_pool_tvl × ratio` の銘柄を candidate から除外
- multi-hop path のボトルネックは最小プール TVL を参照（goal pool 単独ではない）

**期待効果**: メモコイン系プールが構造的に候補から外れる。等分割しても安全圏。

**設計判断**:
- ratio チェックは `apply_liquidity_filter_and_select`（絶対 TVL）でなく
  `portfolio_cost.rs` 側に置く。関心分離（絶対流動性 vs 相対サイズ）。
- `apply_liquidity_filter_and_select` 内の `assumed_in_per_token = total_value × 1/N` は
  N（フィルタ後トークン数）が決まらないと算出不能で循環するため不採用。

**リスク**: 候補トークン数が大幅減 → ポートフォリオの分散が弱まる可能性。
ただし v3 では損失寄与の極小流動性トークンを切ることで全体パフォーマンスは改善する見込み。

### P5. simulate 出力の price impact レポート（観測強化）— データドリブン化

**変更**:
- `crates/simulate/src/output.rs` の `SwapEventEntry` に
  `price_impact_ratio: Option<f64>`（`#[serde(default)]`）フィールド追加
- `crates/simulate/src/portfolio_state.rs::SwapEvent` に同フィールド追加
- `mock_client.rs::handle_swap` 内の **新規 private helper** で
  `to_spot_rate_with_fallback` を sim_day pool で再計算し、no-impact 参照レートを算出
  - 旧計画の「既に market_rate 経路あり」は誤り。実装に存在しないため新規実装が必要
- `(amount_in_near - amount_out_via_no_impact_rate) / amount_in_near` を ratio として記録
  - 内部表現は `f64`（`is_finite()` ガード、負値 = price improvement も保持）
  - JSON フィールド名は `price_impact_ratio`（単位混乱を避けるため `_bps` を含めない）
- パフォーマンスサマリに「平均 price impact」「max price impact」「impact > 100 BPS の swap 数」を集計

**観測 / コスト推定の責務分離**:
- `observe_loss_ratio(input, output) -> f64`: 符号保持（観測用）
- `compute_loss_ratio(input, output) -> f64`: `.max(0.0)` クランプ（コスト推定用、既存維持）

**期待効果**: P1〜P3 の修正前後で impact 統計が定量的に比較できる。

### P6. `trade_count` 表示の修正（report 整合）— 副次

`crates/simulate/src/output.rs:207-211` の `trade_count` は実態 0 固定。
**(a) フィールド削除し `total_swaps` を採用** を推奨。

---

## 5. 実装ステップ（コミット単位、合意ベース 1 PR / 8 コミット）

各ステップ = 1 コミット（CONTRIBUTING.md 準拠）。

### Commit 1: 観測ヘルパ追加（P5 一部）

- `crates/simulate/src/mock_client.rs` 内 private helper として
  `compute_market_rate_at(sim_day, pools, in_token, out_token) -> ExchangeRate` を追加
- `to_spot_rate_with_fallback` を sim_day pool で再計算する経路（既存実装の再利用）
- リスク: なし（観測のみ）
- テスト: `mock_client/tests.rs` に market_rate 計算の unit test 追加

### Commit 2: 観測経路の追加（P5 本体）

- `SwapEvent` / `SwapEventEntry` に `price_impact_ratio: Option<f64>`（`#[serde(default)]`）追加
- `mock_client.rs::handle_swap` で記録
- `PerformanceMetrics` に impact 集計（mean / max / 超過件数）追加
- v3 を再実行して baseline 統計を取得（実装変更前の現状値）
- リスク: なし（観測のみ）
- テスト: `output/tests.rs` に impact 計算の unit test 追加

### Commit 3: `trade_count` 整理（P6）

- `output.rs` から `trade_count` フィールド削除、`total_swaps` を表示に採用
- `main.rs:69-72` のサマリ出力修正
- リスク: 後段消費者があるなら破壊的変更（要確認）
- テスト: 既存 tests の修正

### Commit 4: コスト推定の時刻整合（P3）

- `collect_cost_inputs` シグネチャに `as_of: DateTime<Utc>` 追加（非 Option）
- `read_from_db(None)` → `read_from_db(Some(as_of.naive_utc()))`
- `select_volatility_tokens_inner` 内の `read_from_db(None)` を
  `read_from_db(Some(end_date.naive_utc()))` に修正（1 行）
- `execute_portfolio_strategy` から `params.end_date` を thread
- リスク: 既存呼び出し側の影響範囲。ピアレビュー対象
- テスト: integration（v3 期間で再シミュレーション、Day 1 損失が緩和されるか）

### Commit 5: `TokenSwapBundle` 集約 refactor（P1 準備）

- `paths: BTreeMap` + `rates: BTreeMap` を `bundles: BTreeMap<TokenOutAccount, TokenSwapBundle>` に集約
- `TokenSwapBundle { path: TokenPath, rate: ExchangeRate }`（BUY のみ）
- 既存の let-else 二重ガード（portfolio_cost.rs:175-181）を解消
- refactor only。挙動変更なし。緑テスト維持
- リスク: 並行 BTreeMap → 構造体集約による型変更の波及
- テスト: 既存 tests の通過確認のみ

### Commit 6: SELL path 追加 + 対称コスト計算（P1 本体）

- `TokenSwapBundle` に `sell_path: TokenPath` 追加
- `compute_cost_deductions` で BUY + SELL の variable_ratio を合算
- `graph.update_graph` の SELL 方向対応（`Arc<TokenGraph>` cache 共有 or 双方向 traversal）
  - 実装着手時に O(N × traversal) の許容性を計測。許容なら追加 cache 不要、超過なら refactor
- リスク: **中〜大規模**。graph traversal コストが production 実行時間に影響する可能性
- テスト: cost/tests.rs で BUY+SELL 合算の unit test、integration で v3 再実行

### Commit 7: 差分計算化 + シグネチャ縮小（P2）

- `compute_cost_deductions(weights, inputs, wallet_info: &WalletInfo)` の 3 引数に縮小
- `assumed_in` → `trade_delta_yocto` リネーム
- 計算式を `|target_value - current_value_at_spot|` に変更
- 既存の `total_value` / `tokens` 引数は `wallet_info` から導出
- リスク: シグネチャ変更で呼び出し側の修正必要
- テスト: cost/tests.rs に差分計算の unit test、リバランス時のコスト推定 regression test

### Commit 8: ポジション/プール比率制約（P4）

- `crates/common/src/config/typed.rs` に `TRADE_MAX_POSITION_VS_POOL_RATIO: f64`（既定 0.02）追加
  - `.clamp(0.001, 0.5)` + NaN handler（`clamp_portfolio_pred_err_diagonal_k` パターン）
- `portfolio_cost.rs::compute_cost_deductions` 内で path-wide min TVL を計算
- `trade_delta_yocto[i] > path_min_pool_tvl × ratio` の銘柄を除外
- リスク: 候補トークンが大幅減少した場合、最適化が `Hold` に倒れる可能性 → fallback として閾値の自動緩和も検討
- テスト: simulate 期間で総合パフォーマンス比較（基準: v3 vs 修正後）、sweep [0.01, 0.02, 0.03, 0.05] で sensitivity 確認

### Commit 9（任意・検証ループ）: v3 再シミュレーションと検証レポート

- v3 と同条件で再シミュレーション
- Day 1 即時損失が **-41% → -10% 以下** になることを確認
- 期末累計 P&L が **-56% から大幅改善**することを確認
- 結果を `crates/simulate/docs/verification_report_<date>.md` として残す

### 補足: fail-safe の責務分離（旧 P3 撤回後の代替）

- `compute_variable_ratio` は素直に値を返す（signature 変更なし）
- fail-safe は `run_cost_aware_optimization`（反復ループ）側で
  `(deduction[i] / weight[i])` の推移を観測する形で実装（必要時 Commit 7-8 と同 PR で追加）
- 判定基準は実装フェーズで決定（観測 baseline を取得してから）

### 既存値の見直し（合意済み実装ガードレール）

- `damping` の clamp 下限を `0.0` → **`0.05`** に変更（無限ループ誘発防止）
  - 該当: `crates/trade/src/strategy.rs:938` 付近
  - 別コミットで対応可。本計画と同 PR に含めるかは実装時判断

---

## 6. 期待効果（修正後）

| 修正 | Day 1 損失への寄与 | 期末損失への寄与 |
|---|---|---|
| P1 (対称コスト) | **大**（-161% の主因） | **大** |
| P2 (差分計算) | 中（過剰除外解消） | 中 |
| P3 (時刻整合) | 中（コスト推定の精度） | 中 |
| P4 (比率制約) | **大**（メモコイン除外） | **大** |
| P5 (観測) | なし | なし |
| P6 (表示) | なし | なし |

**最も効果が出る順**: P1 → P4 → P2 → P3。
**実装順序の依存**: P5 (Commit 1-2) で baseline → P3 (Commit 4) → P1 (Commit 5-6) → P2 (Commit 7) → P4 (Commit 8)。

---

## 7. 未解決事項（要検討）

1. **production における `as_of` の供給**
   - `params.end_date` 経路で thread 確定。production の trade ループは `Utc::now()` を渡す
   - `apply_config` 経由は禁忌（process-wide config）

2. **`reverse_paths` の独立探索コスト**
   - `graph.update_graph(start)` は一方向トラバース
   - 各 token を start にした update_graph 呼び出しが必要 → O(N × traversal)
   - `Arc<TokenGraph>` cache 共有 or `update_graph` 双方向対応 refactor が必要かを Commit 6 実装時に検証

3. **シミュレーション期間の代表性**
   - 2026-04-06〜04-16 はメモコイン系の極端な変動期間の可能性
   - 別期間でも同じ問題が再現するかの確認が必要

4. **`rate_calc_near` 静的固定問題（W2）— 別タスク化**
   - `to_spot_rate_with_fallback` の補正式は `Δx = rate_calc_near × 10^24` を使用
   - `rate_calc_near` は record 時点の値で固定
   - record 後にプールが縮小すると、サンプル時の `(x_record + Δx)/x_record` で補正してしまい、
     現在の正しい補正 `(x_now + Δx)/x_now` と乖離
   - 本計画スコープ外。別タスクで対応

5. **`BasisPoints` Newtype 化**
   - 本計画では `Option<f64>` で実装
   - 後続 PR で BPS 系の型を集約導入

6. **既存 `current_time` / `end_date` リネーム**
   - 本計画では既存名を維持
   - 別 PR で機械的リネーム（必要に応じて）

---

## 8. 参考

- `simulation_2026-04-06_to_2026-04-16_v3.json`（基準データ）
- `crates/trade/src/cost.rs`（既存 cost 推定）
- `crates/trade/src/portfolio_cost.rs`（既存 cost-aware 反復ループ）
- `crates/trade/src/strategy.rs:903-955`（cost-aware 経路の入口）
- `crates/persistence/src/token_rate.rs:534-549`（`to_spot_rate_with_fallback`、AMM impact 補正）
- `crates/blockchain/src/ref_finance/path/graph.rs:122`（`update_graph`、一方向トラバース）
- `crates/simulate/src/engine.rs::apply_config`（simulate の設定上書き）
