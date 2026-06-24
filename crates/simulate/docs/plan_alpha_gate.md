# Alpha Gate 計画 (3-reviewer 合意ドラフト) — 保留中

作成日: 2026-05-17
状態: **保留中 (シミュレーション評価基盤の修復が先決)**
理由: simulate が `run_test` DB の 12 日分データで動いており、予測の 87.6% が flat (no signal)。
本 PR の効果検証ができない。fly.io 本番 DB から長期履歴を取り込んでから再評価する。

---

## 1. 背景と根本原因

`feature/all-predicted-token-selection` ブランチで複数戦略改善 (cost-aware, aggregate-cap PR-A, DD breaker PR-B 等) を実装したが、
simulate で **positive return が達成できていない** (最新 30 日 sweep で C0〜C5 全ケース -8.2% 〜 -11.4%)。

### 確定した根本原因 (フェーズ1 + フェーズ2 で確認)

`.tmp/sweep_aggregate_cap/log_c0_baseline_30d.log` の 10 サイクル最適化決定:

| 指標 | 値 |
|---|---|
| 最適化が出す期待リターン (mean) | +0.12%/cycle |
| 期待リターン > 6.7% (実 slippage) | 0/10 サイクル |
| 期待リターンが負のまま rebalance | 4/10 サイクル |
| 実 swap の平均 price impact | 6.7% (max 11.4%) |
| 結果 | -8.2% / 30 日 |

「期待リターン 0.1% を狙って 6.7% スリッページを払う」**構造的逆ザヤ**。

### 追加発見: cost-aware iteration の構造的バグ

`compute_cost_deductions` の pool-ratio cap は `trade_size = |Δw| × total_value`。
cost-aware iteration 初期反復で `weights = 1/N` (N=290) → `Δw ≈ 0.0034` → `trade_size = 0.34 NEAR` という
極小値が cap を素通り。converged target_w (10〜16 NEAR) での実 cost を optimizer が見ていない。

### **本当の根本原因 (本計画保留の理由)**

`run_test` DB の `token_rates` 開始が 2026-03-15 のため、シミュレーション開始 (2026-03-27) 時点で
**全 1025 token が 12 日分未満の履歴しか持たない** (request 30 日 vs 実 12 日)。

- 予測 5504 件中 4400 件 (87.6%) が flat (Δ<0.1%) ← データ不足の正しい反応
- 予測の方向当て精度 47.2% (= coin flip)
- Markowitz が flat × 低分散 → Sharpe=10 と誤判断 → memecoin 集中買い → 6.7% スリッページ損

→ 戦略改善の効果を測れる評価基盤になっていない。
→ fly.io 本番 DB から長期 token_rates を取り込んで再評価する必要あり。

## 2. 合意済の修正計画 (3-reviewer 合意済、評価基盤修復後に着手)

### 2.1 設計骨子

confidence filter 通過後 / Top-N pruning 前に、各候補 token について「フルポジション size での
round-trip cost」を計算し、`H × ER < k × round_trip_cost` の token を除外する。

- 想定サイズ: `total_value × MAX_POSITION_SIZE` (= worst case, **N_target 除算は撤廃**)
- gate 条件式: `H × ER > k × round_trip_cost`

### 2.2 typed config (4 flag)

| key | type | default | clamp | NaN fallback |
|---|---|---|---|---|
| `TRADE_ALPHA_GATE_ENABLED` | bool | false | — | — |
| `TRADE_ALPHA_GATE_MULTIPLIER` | f64 | 2.0 | [0.1, 10.0] | 2.0 |
| `TRADE_ALPHA_GATE_HOLD_CYCLES` | u32 | 1 | [1, 100] | — |
| `TRADE_ALPHA_GATE_MIN_PASS_COUNT` | u32 | 5 | [0, 20] | — |

### 2.3 公開 API (`crates/trade/src/alpha_gate.rs` 新規)

```rust
pub(crate) struct AlphaGateThresholds {
    pub multiplier: f64,
    pub hold_cycles: u32,
    pub min_pass_count: usize,
}

pub(crate) struct AlphaGateOutcome {
    pub kept: HashSet<TokenOutAccount>,          // gate 通過 + fallback 合算
    pub rejected: Vec<AlphaGateRejection>,       // gate で reject (telemetry)
    pub fallback_used: bool,                     // <min_pass_count で発動
    pub fallback_count: usize,                   // fallback 救済数
}

pub(crate) fn apply_alpha_gate(
    tokens: &[TokenData],
    expected_returns: &BTreeMap<TokenOutAccount, f64>,
    cost_inputs: &PortfolioCostInputs,
    total_value_yocto: &BigDecimal,
    thresholds: &AlphaGateThresholds,
    held_tokens: &HashSet<TokenOutAccount>,  // 必須 (Top-N pattern 同様 bypass)
) -> AlphaGateOutcome
```

### 2.4 RoundTripCostRatio Newtype (`crates/trade/src/cost.rs`)

```rust
/// 往復コスト比率 (`(variable × trade + fixed) / trade_size`)
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoundTripCostRatio(f64);

impl RoundTripCostRatio {
    pub(crate) fn new(value: f64) -> Option<Self>  // is_finite() && 0.0 <= value <= 10.0
    pub(crate) fn as_f64(self) -> f64
}

pub(crate) fn estimate_full_position_round_trip_ratio(
    buy_path: &TokenPath, sell_path: &TokenPath,
    position_size: &YoctoValue, spot_rate: &ExchangeRate,
    gas_price: GasPrice, storage_min: &YoctoValue, new_token_count: usize,
) -> Result<RoundTripCostRatio>
```

### 2.5 MAX_POSITION_SIZE 公開

- `crates/common/src/algorithm/portfolio.rs:371` の `const MAX_POSITION_SIZE = 0.6` を **`pub const` 昇格**
- `crates/common/src/algorithm/half_kelly.rs:33` の重複 `const` を**削除**して portfolio の `pub const` を import
- Rule of Three (portfolio + half_kelly + alpha gate) 該当 → DRY 解消の好機

### 2.6 architecture 変更 (CRITICAL 2 件)

#### C1. `wallet_info` 構築の前倒し
- 現状: `strategy.rs:1199-1266` で構築
- 変更: `compute_wallet_info(client, wallet, tokens, period_id, is_new_period, available_funds) -> Result<WalletInfo>`
  ヘルパに抽出、confidence filter 直前で 1 回だけ呼ぶ
- `is_new_period` 分岐 (snapshot/RPC fallback) の二重実行を防ぐ

#### C2. `collect_cost_inputs` の単一呼び出し
- `Option<PortfolioCostInputs>` を local 変数で持ち回し (Arc<…> wrap で clone 軽減)
- alpha gate と cost-aware iteration で同じ snapshot を共有

### 2.7 observability (`CandidateFunnel`)

- `crates/trade/src/candidate_telemetry.rs` の `CandidateFunnel` に `after_alpha_gate: usize` 追加
- フロー: `predicted → after_confidence → after_alpha_gate → after_liquidity → optimizer_input → selected`
- ログ責務マトリクス:
  - `debug!` per-token: gate exclusion ごとに ER と cost を出力
  - `info!` 集約: gate filter の件数と通過数
  - `warn!` 全排除: `"all tokens excluded by alpha gate, holding"`

## 3. コミット構成 (7 commits)

1. **DRY refactor**: `MAX_POSITION_SIZE` を `pub const` 昇格 + `half_kelly.rs` duplicate 削除
2. **typed config**: `TRADE_ALPHA_GATE_*` 4 flag 追加 (`crates/common/src/config/typed.rs`)
3. **Newtype + ヘルパ**: `RoundTripCostRatio` + `estimate_full_position_round_trip_ratio`
   (`crates/trade/src/cost.rs`)
4. **alpha_gate モジュール**: `crates/trade/src/alpha_gate.rs` + `alpha_gate/tests.rs`
   (apply_alpha_gate + AlphaGateOutcome + AlphaGateThresholds)
5. **wallet_info refactor + funnel 拡張**:
   - `compute_wallet_info` ヘルパ抽出 (`strategy.rs`)
   - `CandidateFunnel.after_alpha_gate` 追加 (`candidate_telemetry.rs`)
6. **strategy 統合**: `strategy.rs:1078-1097` 付近に `apply_alpha_gate` 呼び出し挿入
   + integration test (`strategy/tests.rs` 新規)
7. **simulate**: CLI 3 (→2) flag 追加 + apply_config + sweep dimension

## 4. リスク領域 (合意済)

- **(R1) Hold 過剰倒れ**: `min_pass_count=5` の fallback (composite_score 降順) で対象枯渇を防ぐ
- **(R2) held_tokens bypass**: `apply_alpha_gate` の必須引数 (proptest で `held ⊆ result` 保証)
- **(R5) 単位整合**: `H × ER > k × round_trip_cost` で hold_cycles と multiplier を直交分離
- **(R9) `EXPECTED_SLIPPAGE_DEDUCTION=0.005` 過小推定**: 別 PR で `TRADE_EXPECTED_SLIPPAGE_DEDUCTION` 化検討

## 5. 棄却された案

- `TRADE_ALPHA_GATE_POSITION_DIVISOR` (N_target 除算): worst case と整合せず gate threshold が
  6 倍過大評価 → 全 token 排除 → 偽 positive return (Hold 100%) のため **撤廃**
- `pub fn max_position_size()` (関数化): 現存重複の一本化目的に対し過剰、`feedback_future_problems.md`
  抵触 → **`pub const` を採用**
- `TRADE_MAX_POSITION_SIZE` typed config 化: Markowitz box constraint 最深値を env 可変にする
  実用ニーズが本 PR で薄い → **別 follow-up**

## 6. **本計画保留中の理由 — 評価基盤修復が先決**

simulate の前提が崩れている (12 日履歴で予測 87.6% flat) ため、本計画を merge しても
**価値検証ができない**。fly.io 本番 DB から `token_rates` 30+ 日履歴を `run_test` に seed し、
予測を再生成 (`simulate --generate-predictions`) してから baseline を再評価する。

その結果次第:
- baseline が positive → 本 PR は不要 (撤回も検討)
- baseline が依然 negative かつ alpha < cost が確認 → 本 PR の loss reduction 価値を A/B sweep で実証

## 7. 関連ファイル

- `crates/trade/src/strategy.rs:699-1418` `execute_portfolio_strategy`
- `crates/trade/src/portfolio_cost.rs:182-262` `collect_cost_inputs`
- `crates/trade/src/portfolio_cost.rs:330-509` `compute_cost_deductions`
- `crates/trade/src/cost.rs` `estimate_trade_cost`, `CostDeduction`
- `crates/common/src/algorithm/portfolio.rs:371` `MAX_POSITION_SIZE`
- `crates/common/src/algorithm/half_kelly.rs:33` `MAX_POSITION_SIZE` (duplicate)
- `crates/common/src/config/typed.rs:602-639,1076-1080` clamp パターン
- `crates/simulate/src/{cli,engine,sweep}.rs`
- `crates/simulate/docs/plan_price_impact_aware_optimization.md` (前 PR の plan)
