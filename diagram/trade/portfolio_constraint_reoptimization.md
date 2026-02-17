# ポートフォリオ制約最適化統合 — 改修方針

> 対象: `crates/common/src/algorithm/portfolio.rs`
> TODO: L425-427

## 1. 目標アーキテクチャ

> 現行コードでは `maximize_sharpe_ratio()` / `apply_risk_parity()` にボックス制約はなく、
> `apply_constraints()` で max_position を含む全制約を後処理している。
> 本改修により、以下のパイプラインへ移行する。

### パイプライン

```
execute_portfolio_optimization()
  │
  ├─ maximize_sharpe_ratio(max_position)   ... ボックス制約付き厳密解（案C）
  ├─ apply_risk_parity(max_position)       ... clamp 統合（案C）
  ├─ alpha * w_sharpe + (1-alpha) * w_rp   ... ブレンド
  ├─ apply_constraints()                   ... MAX_HOLDINGS + MIN_POSITION_SIZE（離散制約のみ）
  └─ reoptimize_for_survivors()            ... 再最適化 + Sharpe セーフガード（案B）
```

> **注**: `apply_constraints()` から max_position clamp を削除し、離散制約のみに簡素化する。
> ボックス制約は `maximize_sharpe_ratio()` / `apply_risk_parity()` 内部で処理される。

### 関連する定数

| 定数 | 値 | 用途 |
|---|---|---|
| `MAX_POSITION_SIZE` | 0.6 | 最大ポジションサイズ上限 |
| `MIN_POSITION_SIZE` | 0.05 | 最小ポジションサイズ下限 |
| `MAX_HOLDINGS` | 6（設定可能） | 最大保有トークン数 |

`max_position` は `dynamic_max_position()` によりボラティリティに応じて動的に計算される
（`MAX_POSITION_SIZE` 以下の値になる）。

## 2. 問題の分析

### 最適性のずれ

`maximize_sharpe_ratio()` はロングオンリー制約（`w_i >= 0`）のみを組み込んだアクティブセット法で
最適解を算出する。しかし、以下の制約は後処理 (`apply_constraints()`) で適用される:

1. **MAX_HOLDINGS**: 上位6トークン以外をゼロ化
2. **MIN_POSITION_SIZE**: 5%未満のポジションをゼロ化
3. **max_position**: 動的上限を超えるポジションをクランプ

後処理でトークンがゼロ化・クランプされた後、正規化により残存トークンの重みが一律スケーリングされる。
この結果は **残存トークンセットに対する真の最適解ではない**。

### 制約の分類

制約を性質で 2 種類に分類し、それぞれに最適な対処法を適用する:

| 制約 | 種類 | 対処方法 |
|------|------|---------|
| `max_position` | 連続的ボックス制約 | Sharpe/RP 内部に統合（案 C: 厳密解） |
| `MAX_HOLDINGS` | 離散カーディナリティ制約 | 後処理 + 再最適化で回復（案 B） |
| `MIN_POSITION_SIZE` | 離散閾値フィルタ | 後処理 + 再最適化で回復（案 B） |

### 具体例

10トークンで最適化 → `apply_constraints()` で4トークンがゼロ化 → 残り6トークンの重みを正規化。
この6トークン重みは「10トークンの最適解のうち6つを取り出して正規化したもの」であり、
「6トークンだけで最適化した解」とは異なる。

## 3. 改修方針: ハイブリッド（案 C + 案 B 統合）

### 方針の概要

連続的ボックス制約（`max_position`）はアクティブセット法内部で厳密に処理し（案 C）、
離散的制約（`MAX_HOLDINGS`, `MIN_POSITION_SIZE`）は後処理 + セーフガード付き再最適化で回復する（案 B）。

### 選定理由

他の候補アプローチとの比較:

| アプローチ | 概要 | 採否 | 理由 |
|---|---|---|---|
| A. 単純再最適化 | 生存トークンで再度最適化 | × | Sharpe 悪化リスク |
| B. セーフガード付き再最適化 | A + Sharpe 比較で採否判定 | **部分採用** | 離散制約の回復に使用 |
| C. Box 制約 Active Set | アクティブセット法に上限制約を統合 | **部分採用** | 連続的ボックス制約の厳密解 |
| **B+C ハイブリッド** | C で連続制約を内部処理、B で離散制約を回復 | **採用** | 両方の長所を統合 |

## 4. ボックス制約 Active Set アルゴリズム（案 C）

### KKT 条件に基づく 3 集合分割

アクティブセット法を拡張し、各変数を 3 つの集合に分類する:

- **F（free）**: `0 < w_i < max_position` — 内点にある変数
- **L（lower）**: `w_i = 0` — 下限に張り付いた変数
- **U（upper）**: `w_i = max_position` — 上限に張り付いた変数

### KKT 条件の導出

予算制約 `Σw = 1` のラグランジュ乗数を γ とすると、Free 変数に対する KKT 条件は:

```
Σ_FF w_F + Σ_FU w_U = γ μ_excess_F   (μ_excess_F = μ_F - rf)
```

これを w_F について解くと:

```
w_F = γ · Σ_FF⁻¹ · μ_excess_F − Σ_FF⁻¹ · Σ_FU · w_U
    = γp − q

where:
  p = Σ_FF⁻¹ · μ_excess_F   (μ_excess_F = μ_F - rf, 従来と同じ超過リターン)
  q = Σ_FF⁻¹ · Σ_FU · w_U   (上限変数の共分散補正)
```

> **注意**: Σ_FU · w_U の補正は q 側に含まれる。p 側の μ_excess_F は従来通り μ − rf のままであり、
> リターンベクトル自体は補正しない。

### 解法

1. U に属する変数の重みを `max_position` に固定
2. 残り予算: `budget_F = 1.0 − |U| × max_position`
   - `budget_F ≤ 0` の場合（n × max_position < 1.0）: 均等配分 `w_i = 1/n` にフォールバックして終了
3. F に属する変数に対して 2 本の線形ソルブを実行:
   - `p = Σ_FF⁻¹ · μ_excess_F` （μ_excess_F = μ_F − rf, 従来と同じ超過リターン）
   - `q = Σ_FF⁻¹ · (Σ_FU · w_U)` （上限変数の共分散補正）
4. ラグランジュ乗数: `γ = (budget_F + Σq) / Σp`
5. Free 変数の重み: `w_F = γp − q`
6. 違反チェック:
   - `w_i < 0` なら L に移動
   - `w_i > max_position` なら U に移動
   - L/U の変数の KKT 条件違反があれば F に戻す
7. 集合が収束するまで反復

### 後方互換性

U が空のとき（すべての変数が上限未満）、アルゴリズムは現行コードと完全に一致する:
- `budget_F = 1.0`, `μ_excess_F = μ_F`, `q = 0`
- `w_F = γ · Σ_FF⁻¹ · μ_F` （現行の解と同一）

### apply_risk_parity への max_position 統合

現行の `apply_risk_parity()` (portfolio.rs) は反復収束アルゴリズムで重みを算出する。
max_position 制約を以下のように統合する:

1. 各反復ステップで重みを更新した後、正規化の**前**に `w_i = min(w_i, max_position)` で clamp
2. clamp 後に正規化（予算制約 Σw = 1 の回復）
3. clamp → 正規化のサイクルを収束するまで反復

これにより、収束後の解は `0 ≤ w_i ≤ max_position` かつ `Σw = 1` を満たす。

## 5. MAX_HOLDINGS の設定可能化

現行のハードコード定数を設定可能にする:

```
const MAX_HOLDINGS: usize = 6  →  config::get("PORTFOLIO_MAX_HOLDINGS")
```

- デフォルト値: 6（現行と同一）
- 設定ソース: TOML / 環境変数（既存の config パターンに準拠）

## 6. 処理フロー

### 追加する関数

#### `extract_sub_portfolio()`

生存トークンのインデックスから、サブ期待リターンベクトルとサブ共分散行列を抽出するヘルパー。

```
fn extract_sub_portfolio(
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    indices: &[usize],
) -> (Vec<f64>, Array2<f64>)
```

- `indices` で指定された行・列のみを取り出す
- 計算量: O(m²) where m = len(indices) ≤ MAX_HOLDINGS

#### `reoptimize_for_survivors()`

制約適用後の重みベクトルを受け取り、生存トークンのみで再最適化を試みる。

```
fn reoptimize_for_survivors(
    weights: &mut [f64],
    expected_returns: &[f64],
    covariance_matrix: &Array2<f64>,
    alpha: f64,
    max_position: f64,
)
```

### 処理ステップ

```
reoptimize_for_survivors()
  │
  ├─ 1. survivors = { i | weights[i] > 0.0 } を収集
  │
  ├─ 2. 早期リターン判定
  │     ├─ survivors.len() <= 1 → return（1トークンなら 100% 固定）
  │     └─ survivors.len() == weights.len() → return（全トークン生存＝フルセットの最適解）
  │
  ├─ 3. 元の Sharpe を計算
  │     sharpe_original = portfolio_return / portfolio_std
  │
  ├─ 4. サブ問題を構築
  │     (sub_returns, sub_cov) = extract_sub_portfolio(survivors)
  │
  ├─ 5. サブ問題で再最適化（ボックス制約付き）
  │     ├─ w_sharpe_sub = maximize_sharpe_ratio(sub_returns, sub_cov, max_position)
  │     ├─ w_rp_sub = apply_risk_parity(equal_weights, sub_cov, max_position)
  │     └─ w_sub = alpha * w_sharpe_sub + (1-alpha) * w_rp_sub
  │
  ├─ 6. サブ重みに離散制約適用
  │     ├─ MIN_POSITION_SIZE フィルタ
  │     └─ normalize（収束ループ）
  │
  ├─ 7. 元のインデックスに書き戻して候補重みを構築
  │     candidate_weights[survivors[j]] = w_sub[j]
  │
  ├─ 8. 候補の Sharpe を計算
  │     sharpe_candidate = candidate_return / candidate_std
  │
  └─ 9. セーフガード判定
        ├─ sharpe_candidate >= sharpe_original → weights = candidate_weights（採用）
        └─ sharpe_candidate < sharpe_original  → 何もしない（元の重みを維持）
```

### 呼び出し箇所

```rust
// execute_portfolio_optimization() 内:

apply_constraints(&mut optimal_weights);  // 離散制約のみ（max_position は Sharpe/RP 内部で処理済み）

// 生存トークンのみで再最適化（制約適用後の最適性回復）
reoptimize_for_survivors(
    &mut optimal_weights,
    &expected_returns,
    &covariance,
    alpha,
    max_position,
);
```

## 7. テスト戦略

### 追加テスト一覧

| テスト名 | 目的 |
|---|---|
| `test_extract_sub_portfolio` | サブ問題の抽出が正しいか（リターン・共分散の対応関係） |
| `test_reoptimize_preserves_or_improves_sharpe` | 再最適化後に Sharpe が元以上であること |
| `test_reoptimize_safeguard_reverts_on_worse_sharpe` | セーフガード発動時に元の重みが維持されること |
| `test_reoptimize_noop_when_all_survive` | 全トークン生存時に重みが不変であること |
| `test_reoptimize_noop_single_survivor` | 生存トークン1個の場合にスキップされること |
| `test_reoptimize_satisfies_all_constraints` | 再最適化後も全制約（max_position, MIN_POSITION_SIZE, MAX_HOLDINGS）を満たすこと |
| `test_box_constraint_basic` | ボックス制約付き Sharpe 最適化で `w_i ≤ max_position` が成立すること |
| `test_box_constraint_no_effect` | `max_position` が十分大きい場合に制約なしの解と一致すること |
| `test_box_constraint_tight` | `max_position` が小さい場合に複数の変数が上限に張り付くこと |
| `test_box_constraint_budget_infeasible` | `n × max_position < 1.0` の場合にフォールバック（均等配分等）すること |

### テスト設計の方針

- `extract_sub_portfolio` は純粋関数のため、入出力のアサーションで検証可能
- セーフガードのテストでは、意図的に Sharpe が悪化するケースを構築して発動を確認
- 制約テストでは再最適化後の全重みに対して制約条件を網羅的にチェック
- ボックス制約テストでは U が空のケース（後方互換）と非空のケースの両方を検証

## 8. 設計上の注意点

### セーフガードの重要性

セーフガードにより、既存動作を **絶対に悪化させない** ことを保証する。
再最適化で Sharpe が改善した場合のみ新しい重みを採用するため、
最悪でも現状の後処理ベースの結果が維持される。

### 計算コスト

再最適化はサブ問題（n ≤ MAX_HOLDINGS）に対してのみ実施。
`maximize_sharpe_ratio()` のアクティブセット法は n ≤ 6 では実質ゼロコストであり、
パフォーマンスへの影響はない。ボックス制約の追加による反復回数の増加も、
n が小さいため無視できる。

### 再最適化での追加トークン脱落

サブ問題での再最適化結果に MIN_POSITION_SIZE フィルタを適用する際、
追加のトークンが脱落する可能性がある。この場合もセーフガードにより制御される
（Sharpe が改善していれば採用、悪化していれば元の重みを維持）。

### 全トークン生存時のスキップ

全トークンが `apply_constraints()` を生き残った場合、サブ問題はフルセットと同一であり
再最適化の意味がない。早期リターンによりスキップする。

### ボックス制約の後方互換性

ボックス制約 Active Set は、上限に張り付く変数がない場合（U = ∅）に
現行コードと数学的に同一の解を返す。これにより、`max_position` が十分大きい場合の
既存動作が完全に保持される。
