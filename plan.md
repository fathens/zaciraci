# 案 I 実装計画 — 統合ポートフォリオ最適化アルゴリズム

## 対象ファイル

- `crates/common/src/algorithm/portfolio.rs` — メインの実装
- `crates/common/src/algorithm/portfolio/tests.rs` — テスト

## 前提条件

- n=100 を上限とする
- 案 D は実装しない（案 I が `select_optimal_tokens()` と `apply_constraints()` を統合で代替するため）
- Ledoit-Wolf 縮小推定を先行実装する（n=100 では Σ が severely rank-deficient のため前提条件）
- 既存テスト 78 件は全 pass を維持する（回帰防止）

## ステップ一覧

### Step 0: Ledoit-Wolf 縮小推定 — 共分散行列の品質改善

**行数**: ~120 行（関数本体 + 定数）
**場所**: `portfolio.rs` の `calculate_covariance_matrix()` 内部を改修

n=100 では T=29 のデータ点に対して自由パラメータが 5,050 となり、
サンプル共分散行列は severely rank-deficient（ランク上限 28）。
現行の固定正則化（1e-6）では Σ⁻¹ が信頼できないため、
Ledoit-Wolf (2004) の解析的縮小推定で置き換える。

#### 数式

```
Σ_LW = δ · F + (1 - δ) · S

F = (tr(S) / n) · I     // スケーリング単位行列（ターゲット）
S = サンプル共分散行列
δ = 最適縮小係数（解析的に計算）
```

#### δ の解析的計算（Ledoit-Wolf 2004）

```
ledoit_wolf_shrinkage(daily_returns, S) -> (δ, Σ_LW):
  n = S の次元（トークン数）
  T = daily_returns の行数（データ点数）

  // ターゲット: スケーリング単位行列
  mu = tr(S) / n
  F = mu · I

  // δ の解析式に必要な統計量
  // (1) Σ||S - F||² (サンプル共分散とターゲットの二乗距離)
  delta = Σ_{i,j} (S_ij - F_ij)²

  // (2) β̂ の推定（リターン行列から直接計算）
  // 各データ点 t について:
  //   x_t = daily_returns[t] (n 次元ベクトル)
  //   X_t = x_t · x_t' (外積行列)
  //   β̂ += ||X_t - S||² / T²
  beta_hat = (1/T²) · Σ_t ||x_t·x_t' - S||²_F

  // (3) 最適縮小係数
  δ = min(β̂ / delta, 1.0)    // [0, 1] にクランプ

  Σ_LW = δ · F + (1 - δ) · S
  return (δ, Σ_LW)
```

#### 改修箇所

`calculate_covariance_matrix()` (L150-184) を以下のように変更:

```rust
pub fn calculate_covariance_matrix(daily_returns: &[Vec<f64>]) -> Array2<f64> {
    // ... 既存のサンプル共分散計算（L150-173）...

    // 変更: 固定正則化を Ledoit-Wolf 縮小推定に置換
    // Before: covariance[[i, i]] += REGULARIZATION_FACTOR;
    // After:
    let covariance = ledoit_wolf_shrink(daily_returns, covariance);

    ensure_positive_semi_definite(&mut covariance);
    covariance
}
```

新設関数:
```rust
fn ledoit_wolf_shrink(
    daily_returns: &[Vec<f64>],
    sample_cov: Array2<f64>,
) -> Array2<f64>
```

#### テスト

| テスト | 目的 |
|---|---|
| `test_ledoit_wolf_identity_target` | F = (tr(S)/n)·I の正当性 |
| `test_ledoit_wolf_shrinkage_range` | δ ∈ [0, 1] |
| `test_ledoit_wolf_full_rank` | n=50 でも Σ_LW が full rank |
| `test_ledoit_wolf_backward_compat` | n=8 で既存テストが pass（δ は小さく、S に近い） |
| `test_ledoit_wolf_well_conditioned` | 条件数が合理的な範囲に収まる |

#### 注意

- Ledoit-Wolf 適用後も `ensure_positive_semi_definite()` は残す（防御的）
- `REGULARIZATION_FACTOR` の対角加算は削除（Ledoit-Wolf が代替）
- n ≤ 10 では δ が小さくなり、サンプル共分散に近い結果 → 既存動作と整合

---

### Step 1: `box_maximize_sharpe()` — ボックス制約付き Sharpe 最大化

**行数**: ~100 行
**場所**: `portfolio.rs` の `maximize_sharpe_ratio()` の後に新設

Section 4 設計の 3 集合 Active Set 法を実装する。

```rust
pub fn box_maximize_sharpe(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    max_position: f64,
) -> Vec<f64>
```

**アルゴリズム**:
1. 事前チェック: `n × max_position < 1.0` → `effective_max = max(max_position, 1/n)` で等配分返却
2. 3 集合 (F/L/U) を管理。初期: 全て F
3. Free 変数に対して Σ_FF⁻¹ · μ_excess_F を解く（Cholesky/LU）
4. U 集合からの共分散補正 q = Σ_FF⁻¹ · Σ_FU · w_U
5. ラグランジュ乗数 γ = (budget_F + Σq) / Σp
6. 違反チェック: F→L (w<0), F→U (w>max_pos), L→F / U→F (勾配条件)
7. 最大 3n 反復で収束

**互換性**: `max_position >= 1.0` のとき既存 `maximize_sharpe_ratio()` と同一の解を返す。

**n=100 での注意**: Free 集合のサイズが大きい場合 Σ_FF の Cholesky 分解がドミナント。
Ledoit-Wolf 後の Σ は well-conditioned なので Cholesky は安定して成功する見込み。

---

### Step 2: `box_risk_parity()` — ボックス制約付き Risk Parity

**行数**: ~60 行
**場所**: `apply_risk_parity()` の後に新設

Section 4 設計の固定集合法を実装する。

```rust
pub fn box_risk_parity(
    covariance_matrix: &Array2<f64>,
    max_position: f64,
) -> Vec<f64>
```

**アルゴリズム**:
1. Free/Pinned 集合管理。初期: 全て Free、等配分
2. Free 集合のみで RP 反復（budget = 1.0 - |Pinned| × max_position）
3. Free→Pinned: w_i > max_position のトークンを pin
4. Pinned→Free: RC_i > target_RC のトークンを unpin
5. 最大 2n 反復で収束

**戻り値**: 正規化済み重みベクトル（合計 1.0、全要素 ∈ [0, max_position]）

**n=100 での注意**: RP 反復の max 回数は 2n=200。各反復で Σ·w の計算 O(n²) が必要。
合計 O(n³) で Phase 1 全体に十分含まれる。

---

### Step 3: ユーティリティ関数群

**行数**: ~100 行

#### 3a. `extract_sub_portfolio()` (~20 行)
```rust
fn extract_sub_portfolio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    indices: &[usize],
) -> (Vec<f64>, Array2<f64>)
```
指定インデックスのサブ μ, サブ Σ を抽出。

#### 3b. `risk_parity_divergence()` (~15 行)
```rust
fn risk_parity_divergence(weights: &[f64], covariance_matrix: &Array2<f64>) -> f64
```
RC 均等度の計算: `mean((RC_i - target)²)`

#### 3c. `adjust_returns_for_liquidity()` (~15 行)
```rust
fn adjust_returns_for_liquidity(
    expected_returns: &[f64],
    liquidity_scores: &[f64],
    lambda: f64,
) -> Vec<f64>
```
`μ_adj[i] = μ[i] - λ * (1.0 - liquidity[i])`

#### 3d. `hard_filter_tokens()` (~25 行)
```rust
fn hard_filter_tokens(
    tokens: &[TokenData],
    predictions: &BTreeMap<TokenOutAccount, TokenPrice>,
    historical_prices: &BTreeMap<TokenOutAccount, PriceHistory>,
) -> Vec<TokenData>
```
`select_optimal_tokens()` のフィルタ部分（流動性 + 時価総額）を抽出。
スコアリングや相関フィルタは行わない。

#### 3e. 組み合わせイテレータ (~25 行)
```rust
fn combinations(n: usize, k: usize) -> impl Iterator<Item = Vec<usize>>
```
C(n, k) の全列挙。再帰なし、辞書式順序。

---

### Step 4: `exhaustive_optimize()` — Phase 3 全列挙

**行数**: ~80 行
**場所**: ユーティリティ関数の後に新設

```rust
fn exhaustive_optimize(
    active_indices: &[usize],
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    max_position: f64,
    max_holdings: usize,
    min_position_size: f64,
    alpha: f64,
) -> Vec<f64>
```

**アルゴリズム**:
1. `len(active) <= max_holdings` → `box_blend_optimize` で直接返却
2. `combinations(active, max_holdings)` の全列挙
3. 各サブセット: `box_maximize_sharpe` + `box_risk_parity` + alpha ブレンド
4. MIN_POSITION_SIZE 違反 → 生存トークンで再最適化
5. 複合スコア `alpha * sharpe - (1-alpha) * rp_div` で最良を選択
6. フォールバック: 等配分

**Phase 3 のサブセットは k=6 の極小問題なので n=100 でも影響なし。**

---

### Step 5: `unified_optimize()` — 3 フェーズ統合

**行数**: ~130 行
**場所**: `execute_portfolio_optimization()` の前に新設

```rust
fn unified_optimize(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    liquidity_scores: &[f64],
    max_position: f64,
    max_holdings: usize,
    min_position_size: f64,
    alpha: f64,
    lambda: f64,
) -> Vec<f64>
```

**アルゴリズム**:
1. 流動性調整リターン μ_adj を計算
2. **Phase 1**: 全 n トークンで `box_maximize_sharpe` + `box_risk_parity`
3. **Phase 2**: Sharpe 上位 PRUNE_KEEP_PER ∪ RP 上位 PRUNE_KEEP_PER の和集合で枝刈り
4. **Phase 3**: `exhaustive_optimize` で厳密解

**新規定数**:
```rust
const PRUNE_KEEP_PER: usize = 12;  // 2 × MAX_HOLDINGS
const LIQUIDITY_PENALTY_LAMBDA: f64 = 0.01;
```

**n=100 での計算量**: Phase 1 で 100×100 の box 最適化が 2 回（Sharpe + RP）。
Ledoit-Wolf 後の Σ は well-conditioned なので安定。
Phase 2 で和集合最大 24 トークン。Phase 3 で C(24,6)=134,596 の 6×6 問題。
合計ミリ秒〜数十ミリ秒。

---

### Step 6: `execute_portfolio_optimization()` の改修

**変更内容**: 既存パイプラインを `unified_optimize()` に置き換え。

**Before** (L875-953):
```
select_optimal_tokens(MAX_HOLDINGS + 2)
 → maximize_sharpe_ratio()
 → apply_risk_parity()
 → alpha blend
 → apply_constraints()
```

**After**:
```
hard_filter_tokens()
 → unified_optimize()   // 内部で全てを処理
```

具体的な変更:
1. `select_optimal_tokens()` 呼び出しを `hard_filter_tokens()` に置換
2. L899-953 の一連の処理（Sharpe → RP → blend → constraints）を `unified_optimize()` 1 行に置換
3. alpha 計算ロジック（L932-944）は `unified_optimize()` の引数として渡す
4. `dynamic_max_position()` は引き続き外で計算して渡す
5. L956 以降（リバランス判定、メトリクス計算）は変更なし

---

### Step 7: テスト

**新規テスト** (~400 行):

#### Step 0 (Ledoit-Wolf) テスト

| テスト | 目的 |
|---|---|
| `test_ledoit_wolf_identity_target` | F = (tr(S)/n)·I の正当性 |
| `test_ledoit_wolf_shrinkage_range` | δ ∈ [0, 1] |
| `test_ledoit_wolf_full_rank` | n=50 でも Σ_LW が full rank |
| `test_ledoit_wolf_backward_compat` | n=8 で既存テストが pass |
| `test_ledoit_wolf_well_conditioned` | 条件数が合理的範囲 |

#### Step 1-6 テスト

| テスト | 目的 |
|---|---|
| `test_box_sharpe_basic` | w_i ≤ max_position の制約充足 |
| `test_box_sharpe_backward_compat` | max_position=1.0 で既存 `maximize_sharpe_ratio` と同一解 |
| `test_box_sharpe_n100` | n=100 での動作・制約充足 |
| `test_box_rp_basic` | box 制約付き RP の制約充足 |
| `test_box_rp_n100` | n=100 での RP 動作 |
| `test_extract_sub_portfolio` | サブ問題抽出の正当性 |
| `test_risk_parity_divergence` | RC 均等度の計算 |
| `test_unified_small_n` | n ≤ max_holdings でエッジケース処理 |
| `test_unified_medium_n` | n=10 での動作 |
| `test_unified_large_n` | n=50-100 での動作・計算時間 |
| `test_unified_all_constraints_satisfied` | 全制約充足（box + max_holdings + min_position） |
| `test_pruning_union_preserves_top_tokens` | 和集合枝刈りの正当性 |
| `test_liquidity_adjustment` | 流動性ペナルティ効果 |
| `test_composite_score_consistency` | 複合スコアと Sharpe+RP ブレンド目的の整合 |
| `test_min_position_reoptimization` | MIN_POSITION_SIZE 後の再最適化で制約充足 |
| `test_exhaustive_vs_current_pipeline` | 現行パイプラインとの回帰テスト |
| `test_combinations_iterator` | C(n,k) 列挙の正当性 |
| `test_hard_filter_tokens` | ハードフィルタが既存フィルタと一致 |

**既存テスト**: 全 78 件が pass することを確認。
特に `test_execute_portfolio_optimization` が統合後も pass することが重要。

---

## 実装順序と依存関係

```
Step 0: Ledoit-Wolf              ← 最優先（n=100 の前提条件、既存コードの改修）
Step 1: box_maximize_sharpe      ← Step 0 完了後（Σ の品質に依存）
Step 2: box_risk_parity          ← Step 0 完了後（Step 1 と並行可能）
Step 3: ユーティリティ群          ← 独立（Step 0 と並行可能）
Step 4: exhaustive_optimize      ← Step 1, 2, 3 に依存
Step 5: unified_optimize         ← Step 1, 2, 3, 4 に依存
Step 6: execute 改修             ← Step 5 に依存
Step 7: テスト                   ← 各 Step 完了後に逐次追加
```

Step 0 が完了すれば Step 1-3 は並行実装可能。
Step 4-6 は順番に積む。

## リスクと対策

| リスク | 対策 |
|---|---|
| 既存テスト破壊 | Step 0 (Ledoit-Wolf) は `calculate_covariance_matrix` の内部改修のみ。既存テスト 78 件の pass を確認してから Step 1 以降に進む |
| Ledoit-Wolf の δ 計算誤り | δ=0（縮小なし）と δ=1（完全ターゲット）の境界テスト。n=8 での既存動作との差分が微小であることを検証 |
| n=100 での Cholesky 分解失敗 | Ledoit-Wolf 後の Σ_LW は well-conditioned。それでも失敗した場合は LU 分解にフォールバック（既存コードと同様） |
| box Active Set の収束不良 | 安全弁 3n 反復 + 等配分へのフォールバック |
| Phase 3 の計算量が予想超 | PRUNE_KEEP_PER を下げて対応（12→9 等）。最大 C(24,6)=134,596 で数十ミリ秒以内 |
| 複合スコアの rp_div スケール | alpha=1.0 での Sharpe 単体テストで既存動作と比較 |
| n=100 のテスト用ダミーデータ生成 | ランダムシード固定で n=100 の合成リターンデータを生成。再現可能性を保証 |
