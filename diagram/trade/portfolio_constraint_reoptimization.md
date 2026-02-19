# ポートフォリオ制約最適化統合 — 改修方針

> 対象: `crates/common/src/algorithm/portfolio.rs`
> TODO: L425-427

> **結論**: 本文書は案 B+C（ボックス制約 Active Set + セーフガード付き再最適化）と
> 案 D（トークン選定改善）を比較検討し、実装コスト対効果と入力推定誤差の支配性から
> **案 D を採用**する。Sections 1-9 は案 B+C の詳細設計（参考資料）、
> Section 10 が採用する案 D の設計である。

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
  ├─ apply_constraints()                   ... 離散制約 + 防御的 box 強制
  └─ reoptimize_for_survivors()            ... 再最適化 + Sharpe セーフガード（案B）
```

> **変更点**: `apply_constraints()` Phase 1 から max_position 初期 clamp を削除する。
> ボックス制約は `maximize_sharpe_ratio()` / `apply_risk_parity()` 内部で最適に処理される。
> ただし Phase 2（防御的 clamp → normalize ループ）は残す。
> 離散制約（MAX_HOLDINGS / MIN_POSITION_SIZE）適用後の normalize で
> box 制約違反が起きうるため、事後的な box 強制が必要。

### `apply_constraints()`（改修後）の擬似コード

```
apply_constraints()（改修後）:

  Phase 1（離散制約ループ、最大10回）:
    1. 非負 clamp: max(w_i, 0.0)（normalize 後の浮動小数点誤差で負になりうるため）
    2. MAX_HOLDINGS フィルタ（上位 N 以外をゼロ化）
    3. MIN_POSITION_SIZE フィルタ（閾値未満をゼロ化）
    4. normalize（合計 = 1.0）
    5. 変更なしなら break

  Phase 2（防御的 box ループ、最大10回）:
    1. clamp [0.0, max_position]
    2. normalize
    3. 変更なしなら break
```

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

0. **事前チェック**: `n × max_position < 1.0`（大域的非実行可能ケース）の場合:
   - `effective_max = max(max_position, 1/n)` に緩和し、均等配分 `w_i = 1/n` で即座に返す
   - **理由**: 全変数を上限にしても予算 `Σw = 1` を満たせないため、
     ボックス制約を予算制約に劣後させる設計判断。`1/n > max_position` なので
     均等配分は元の max_position を超えるが、予算制約の充足を優先する。
   - **代替案**: 呼び出し元で `max_position = max(max_position, 1/n)` に調整してから渡す
1. U に属する変数の重みを `max_position` に固定
2. 残り予算: `budget_F = 1.0 − |U| × max_position`
   - `budget_F ≤ 0` の場合: U の変数だけで予算を消費しており F に配分できない。
     U の中でリターンが最も低い変数を F に戻し、Step 1 から再実行する
     （外側ループの次の反復として処理される）。
3. F に属する変数に対して 2 本の線形ソルブを実行:
   - `p = Σ_FF⁻¹ · μ_excess_F` （μ_excess_F = μ_F − rf, 従来と同じ超過リターン）
   - `q = Σ_FF⁻¹ · (Σ_FU · w_U)` （上限変数の共分散補正）
4. ラグランジュ乗数: `γ = (budget_F + Σq) / Σp`
5. Free 変数の重み: `w_F = γp − q`
6. 違反チェック（ε = 1e-10 の許容誤差を使用）:
   - `w_i < -ε` なら L に移動（微小な負値は 0 にクランプして F に留める）
   - `w_i > max_position + ε` なら U に移動
   - L の変数 i: `∂L/∂w_i > ε`（重みを増やすと改善）なら F に戻す
   - U の変数 i: `∂L/∂w_i < -ε`（重みを減らすと改善）なら F に戻す
   - 勾配: `∂L/∂w_i = γ · μ_excess_i − (Σw)_i`
   - **注意**: 浮動小数点演算では `w_i = -1e-15` のような微小な負値が発生しうる。
     許容誤差なしでは不要な集合移動が発生し、収束が遅延する可能性がある。
   **処理順序**: 1反復につき最も違反の大きい変数を1つだけ移動する。
   - F → L: 最も負の w_i を移動
   - F → U: 最も上限超過量の大きい w_i を移動
   - L → F / U → F: 勾配の絶対値が最大の変数を移動
   - F→L/U（実行不可能性の解消）を L/U→F（最適性の改善）より優先する。
7. 集合が収束するまで反復

### 停止性

各反復で少なくとも 1 つの変数が集合間を移動する。変数 × 集合の組み合わせは有限
（各変数は F/L/U の 3 状態、最悪 3^n 通り）なので、同一状態への再訪がなければ
有限回で停止する。実用上は n ≤ 10 程度のため反復回数は問題にならない。

**Anti-cycling 安全弁**: 最大反復回数を `3 × n` に設定する。
n ≤ 10 では最大 30 回の反復で十分であり、万一の無限ループを防止する。
最大反復回数に達した場合は、その時点の解（F の変数に対する最新の w_F）を返す。

### 後方互換性

U が空のとき（すべての変数が上限未満）、アルゴリズムは現行コードと完全に一致する:
- `budget_F = 1.0`, `μ_excess_F = μ_F`, `q = 0`
- `w_F = γ · Σ_FF⁻¹ · μ_F` （現行の解と同一）

### apply_risk_parity への max_position 統合

現行の `apply_risk_parity()` (portfolio.rs:368-418) は反復収束アルゴリズムで重みを算出する。

**単純な clamp → normalize の問題点**: 各反復で `w_i = min(w_i, max_position)` → normalize
を行うと、normalize が clamp 済みの重みを再び max_position 以上に押し上げうる。
例: `[0.5, 0.3, 0.2]` に `max_position = 0.4` → clamp `[0.4, 0.3, 0.2]` → normalize
`[0.444, 0.333, 0.222]` → 再違反。内側ループが必要になり収束が遅い。

**固定集合法**（Sharpe のアクティブセットと同様のアプローチ）:

1. 全トークンを free 集合に初期化
2. Free 集合のトークンに対して RP 反復を実行（予算 = `1.0 − |pinned| × max_position`）

> **シグネチャ変更**: 固定集合法は `apply_risk_parity` 内部に統合する。
> 外側ループでの free 集合の正規化先を `budget_F = 1.0 − |pinned| × max_position`
> とするため、正規化処理を修正する（現行は常に合計 1.0 に正規化）。
> シグネチャ: `fn apply_risk_parity(weights: &mut [f64], covariance_matrix: &Array2<f64>, max_position: f64)`
3. 収束後、全トークンの重み（free + pinned）でポートフォリオリスクを計算:
   - `σ_p = sqrt(w' Σ w)`, `target_RC = σ_p / n`（n = 全トークン数）
4. Free → Pinned: `w_i > max_position` のトークンを pinned に移動し `max_position` に固定
5. Pinned → Free: pinned トークン i のリスク寄与度 `RC_i = max_position · (Σw)_i / σ_p` が
   `target_RC` を超える場合、free に戻す（RP が重みを減らしたい = 上限固定が不適切）
6. 集合が安定するまで 2-5 を反復

これにより、各サブ問題は box 制約を満たした状態で RP 収束し、
外側ループも有限回で停止する（Sharpe の場合と同様の停止性保証）。

**停止性**: 最大反復回数を `2 × n` に設定する。

> **注意**: Sharpe の場合（二次計画法）は各反復で目的関数が厳密に改善されるため、
> 同一状態への再訪がないことが理論的に保証される。しかし RP の目的関数は
> リスク寄与度の均等化（非二次）であり、同一の厳密な停止性証明は適用できない。
> 理論上、Pinned → Free → RP 収束 → 再び max_position 超過 → Pinned に戻る
> という振動の可能性が排除されていない。
>
> ただし n ≤ 6 の小規模問題では実用上問題にならず、`2 × n` の最大反復回数が
> 安全弁として機能する。最大反復回数に達した場合は、その時点の解を返す。

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

#### `apply_box_constraint_loop()`

Phase 2 相当の防御的 box 制約ループ。サブ問題での制約適用に使用する。

```
fn apply_box_constraint_loop(weights: &mut [f64], max_position: f64)
```

- clamp [0.0, max_position] → normalize を最大10回反復
- `apply_constraints()` の Phase 2 と同一のロジック

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
  │     ├─ w_sub = alpha * w_sharpe_sub + (1-alpha) * w_rp_sub
  │     └─ 注意: maximize_sharpe_ratio が共分散行列の特異性等で失敗した場合、
  │           均等配分にフォールバックする（現行コードの既存動作）。
  │           この場合、セーフガード（Step 9）により元の重みが維持されるため安全。
  │
  ├─ 6. サブ問題に防御的 box 制約のみ適用
  │     ├─ Phase 2（clamp [0, max_position] → normalize ループ）のみ実行
  │     ├─ Phase 1 の離散フィルタはスキップ
  │     │   理由: survivors は既に MAX_HOLDINGS / MIN_POSITION_SIZE を通過済み。
  │     │   サブ問題で MIN_POSITION_SIZE 未満のトークンが生じた場合は、
  │     │   セーフガード（Step 9）による Sharpe 比較で制御する。
  │     └─ ヘルパー関数 apply_box_constraint_loop(weights, max_position) を新設
  │
  ├─ 7. 元のインデックスに書き戻して候補重みを構築
  │     candidate_weights[survivors[j]] = w_sub[j]
  │
  ├─ 8. 候補の Sharpe を計算
  │     sharpe_candidate = candidate_return / candidate_std
  │
  └─ 9. セーフガード判定
        ├─ sharpe_original >= 0 または sharpe_candidate >= 0:
        │     Sharpe 比較: sharpe_candidate >= sharpe_original なら採用
        └─ 両方 < 0（超過リターンが負）:
              Sharpe の大小比較は反直感的になりうるため、リターン直接比較で判定:
              candidate_return > original_return かつ
              candidate_std ≤ original_std × 1.1 なら採用
              （リターン改善かつリスク大幅増でない場合のみ）
```

### 呼び出し箇所

```rust
// execute_portfolio_optimization() 内:

// Phase 1 から初期 clamp を除去（Sharpe/RP 内部で処理済み）
// Phase 2 の防御的 clamp → normalize ループは維持
apply_constraints(&mut optimal_weights, max_position);

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
| `test_box_constraint_budget_infeasible` | `n × max_position < 1.0` の大域的非実行可能ケースで均等配分にフォールバックすること |
| `test_risk_parity_box_constraint` | RP + box 制約で `w_i ≤ max_position` かつリスク寄与度が均等化されること |
| `test_risk_parity_box_no_effect` | `max_position` が十分大きい場合に制約なし RP と一致すること |
| `test_apply_box_constraint_loop` | Phase 2 相当の box clamp + normalize ループが正しく収束すること |

### テスト設計の方針

- `extract_sub_portfolio` は純粋関数のため、入出力のアサーションで検証可能
- セーフガードのテストでは、意図的に Sharpe が悪化するケースを構築して発動を確認
- 制約テストでは再最適化後の全重みに対して制約条件を網羅的にチェック
- ボックス制約テストでは U が空のケース（後方互換）と非空のケースの両方を検証
- RP テストでは固定集合法の収束と、pinned 変数が max_position に張り付くことを検証

## 8. 設計上の注意点

### セーフガードの重要性

セーフガードにより、既存動作を **絶対に悪化させない** ことを保証する。
再最適化で Sharpe が改善した場合のみ新しい重みを採用するため、
最悪でも現状の後処理ベースの結果が維持される。

**負の Sharpe ratio の扱い**: 元の Sharpe と候補の Sharpe が共に負の場合（超過リターンが負）、
Sharpe の大小比較は反直感的になりうる。
例えば、ボラティリティが高いほど Sharpe が 0 に近づき「良い」と判定されるが、
本質的にリスクが高い。この問題を回避するため、両者が共に負の場合は
リターン直接比較で判定する: `candidate_return > original_return` かつ
`candidate_std ≤ original_std × 1.1` なら採用（リターン改善かつリスク大幅増でない場合のみ）。

### 計算コスト

再最適化はサブ問題（n ≤ MAX_HOLDINGS）に対してのみ実施。
`maximize_sharpe_ratio()` のアクティブセット法は n ≤ 6 では実質ゼロコストであり、
パフォーマンスへの影響はない。ボックス制約の追加による反復回数の増加も、
n が小さいため無視できる。

### 再最適化での追加トークン脱落

サブ問題での再最適化結果に `apply_constraints()` を適用する際、
追加のトークンが脱落する可能性がある。この場合もセーフガードにより制御される
（Sharpe が改善していれば採用、悪化していれば元の重みを維持）。

**再帰的 reoptimize を行わない理由**: 追加脱落後にさらに `reoptimize_for_survivors()`
を再帰的に呼ぶことで最適性を向上できる余地はあるが、以下の理由で行わない:
1. n ≤ 6 の小規模問題では追加脱落自体が稀
2. セーフガードが最悪ケース（Sharpe 悪化）を防止する
3. 再帰的呼び出しはコードの複雑化と停止性の保証を困難にする

### 全トークン生存時のスキップ

全トークンが `apply_constraints()` を生き残った場合、サブ問題はフルセットと同一であり
再最適化の意味がない。早期リターンによりスキップする。

### ボックス制約の後方互換性

ボックス制約 Active Set は、上限に張り付く変数がない場合（U = ∅）に
現行コードと数学的に同一の解を返す。これにより、`max_position` が十分大きい場合の
既存動作が完全に保持される。

### ブレンドの box 制約保存

`maximize_sharpe_ratio` と `apply_risk_parity` が個別に box 制約を満たすなら、
凸結合 `α·w_sharpe + (1-α)·w_rp` も自動的に box 制約を満たす:

- 上限: `α·w_i + (1-α)·w_i' ≤ α·max_position + (1-α)·max_position = max_position`
- 下限・予算制約も同様（凸結合の性質）

したがってブレンド直後には box 違反は起きない。
防御的 clamp は離散制約の normalize 後にのみ必要。

## 9. 実装ステップ

1. `maximize_sharpe_ratio(expected_returns, covariance_matrix, max_position)` — シグネチャ変更 + ボックス制約アクティブセット実装
2. `apply_risk_parity(weights, covariance_matrix, max_position)` — シグネチャ変更 + 固定集合法実装
3. `apply_constraints()` — Phase 1 から box clamp を除去（非負 clamp のみ残す）
4. `apply_box_constraint_loop(weights, max_position)` — Phase 2 相当のヘルパー新設
5. `extract_sub_portfolio()` — サブ問題抽出ヘルパー新設
6. `reoptimize_for_survivors()` — 再最適化関数新設
7. `execute_portfolio_optimization()` — 呼び出し修正（max_position を Sharpe/RP に渡す + reoptimize 呼び出し追加）
8. `MAX_HOLDINGS` — 設定可能化（config パターンに準拠）
9. テスト追加（Section 7 の一覧に従う）

## 10. 代替案 D — トークン選定改善アプローチ

> 案 B+C ハイブリッドのレビューを経て、より軽量な代替案を検討した結果、
> 以下のアプローチを採用する。

### 方針概要

**根本原因への対処**: 現行の最適性ロスの主因は、`select_optimal_tokens()` が
`MAX_HOLDINGS + 2`（= 8）トークンを選定し、最適化後に `apply_constraints()` の
MAX_HOLDINGS フィルタが 2 トークンを脱落させることにある。
脱落したトークンの重みは残存トークンに normalize で一律再配分されるため、
残存トークンセットに対する真の最適解から乖離する。

案 D は **脱落自体を防ぐ** ことで、後処理の再最適化を不要にする:

1. **バッファ削減**: `max_tokens` を `MAX_HOLDINGS + 2` → `MAX_HOLDINGS` に変更
2. **相関フィルタの最小相関優先フォールバック**: バッファ削減で候補不足が起きうるリスクを緩和

**トレードオフ: 除外判断の主体変更**

現行では optimizer が共分散構造を考慮して重み下位 2 トークンを除外するが、
案 D ではヒューリスティックスコア順で事前に除外する。
`select_uncorrelated_tokens` の貪欲法はスコア降順で選択するため、
`max_tokens=6` と `max_tokens=8` で最初の 6 トークンは同一であり、
案 D は「7, 8 番目のトークンを optimizer に渡さない」ことと等価。

optimizer の重み順位とヒューリスティックスコア順位が一致しないケース
（例: スコアは低いが分散投資効果で重みが大きいトークン）では、
案 D は異なるトークンセットで最適化することになる。

ただし、相関フィルタが部分的に共分散構造をカバーしており、
かつ入力推定誤差が支配的であるため、この差異は実用上無視できる。

### 案 B+C との比較と選定理由

| 観点 | 案 B+C ハイブリッド | 案 D トークン選定改善 |
|------|---------------------|----------------------|
| 実装ステップ | 9 ステップ | 2 ステップ |
| 変更関数数 | 6 関数（新設 3 + 改修 3） | 2 関数（改修のみ） |
| 影響範囲 | Sharpe/RP/constraints/reoptimize | select_optimal_tokens 周辺のみ |
| 数学的正当性 | ボックス制約 Active Set は厳密 | 入力段での候補選定変更 |
| RP 停止性 | 理論的保証なし（安全弁で対処） | 変更なし（既存コードを維持） |
| テスト工数 | 13 テスト新設 | 5 テスト新設 |
| 残存する最適性ロス | なし（厳密解） | MIN_POSITION_SIZE による脱落のみ |
| 入力推定誤差への感度 | 高精度な解を出すが入力誤差が支配的 | 入力誤差の範囲内で十分 |

**選定理由**:

1. **入力の不確実性が支配的**: 期待リターン・共分散行列の推定誤差は、
   後処理の最適性ロスよりはるかに大きい。案 B+C の厳密解は理論的に優れるが、
   入力推定の精度が律速となるため、実用上の改善幅は限定的。

2. **RP 固定集合法の停止性リスク**: Section 4 が認めるように、RP の固定集合法は
   二次計画法と異なり理論的な停止性保証がない。安全弁（最大反復回数）で対処するが、
   未知の振動パターンが存在するリスクは排除できない。

3. **実装コスト対効果**: 9 ステップ・6 関数の大規模改修に対し、n ≤ 6 の
   小規模問題での改善幅が限定的。2 ステップ・2 関数の改修で主要な問題を解決できる。

4. **根本原因への直接対処**: MAX_HOLDINGS フィルタによる脱落が最大の最適性ロス源であり、
   脱落自体を防げば再最適化は不要になる。

### Step 1: バッファ削減

#### 変更内容

`execute_portfolio_optimization()` での `select_optimal_tokens()` 呼び出しを変更:

```rust
// 変更前（portfolio.rs:879）
MAX_HOLDINGS + 2, // 相関フィルタでの除外余地を含むバッファ

// 変更後
MAX_HOLDINGS, // MAX_HOLDINGS フィルタの脱落を防止
```

#### 効果

- `select_optimal_tokens()` が返すトークン数 ≤ `MAX_HOLDINGS`
- `apply_constraints()` Phase 1 の MAX_HOLDINGS フィルタが **no-op** になる
  （候補数が既に MAX_HOLDINGS 以下のため、脱落が発生しない）
- 最も大きな最適性ロス（2 トークン脱落 + normalize の歪み）が解消される

#### バッファの元の意図との整合性

元のバッファ（+2）は「相関フィルタでの除外余地」を意図していた。
しかし実際には、相関フィルタは `select_uncorrelated_tokens()` 内部で
`max_tokens` を上限として貪欲選択を行うため、バッファは
「相関フィルタに余裕を与える」のではなく「余分なトークンを後段に渡す」効果しかなかった。
Step 2 の最小相関優先フォールバックにより、バッファなしでも十分なトークン数を確保できる。

### Step 2: 相関フィルタの最小相関優先フォールバック

#### 問題

バッファ削減により `max_tokens` が 8 → 6 に減る。相関フィルタの閾値 0.85 が
厳しすぎる場合、`max_tokens` 個のトークンを選定できず、候補不足に陥るリスクがある。

#### 変更内容

`select_uncorrelated_tokens()` を 2 パス方式に変更:

```
select_uncorrelated_tokens(scored_tokens, historical_prices, max_tokens):

  // 1パス目: 現行閾値で貪欲選択
  selected = greedy_select(scored_tokens, threshold=0.85, max_tokens)

  // 2パス目（不足時のみ）: 最小相関優先で追加選択
  if selected.len() < max_tokens:
    remaining = scored_tokens - selected
    // 各候補の「既存選択との最大絶対相関」を計算
    remaining_with_corr = [(token, max |corr(token, s)| for s in selected) for token in remaining]
    // 最大相関の昇順でソート（最も独立性が高い候補を優先）
    remaining_with_corr.sort_by(|a, b| a.max_corr.cmp(b.max_corr))
    for (token, max_corr) in remaining_with_corr:
      if selected.len() >= max_tokens:
        break
      if max_corr < NEAR_DUPLICATE_THRESHOLD:  // 0.98: ほぼ同一資産を除外
        selected.push(token)

  return selected
```

#### 2 パスでも max_tokens に到達しない場合

市場のトークン数が少ない、または大半が高相関の場合、2 パス目でも
`max_tokens` 個のトークンを確保できないことがある。
ただし 2 パス目で除外されるのは `NEAR_DUPLICATE_THRESHOLD`（0.98）以上の
ほぼ同一資産のみであり、現行の固定閾値方式よりも到達可能性は高い。
それでも不足する場合は利用可能なトークンをそのまま返す（現行動作と同一）。
downstream の最適化処理は任意のトークン数で動作するため、問題は生じない。

#### 設計判断

| 項目 | 決定 | 理由 |
|------|------|------|
| 2パス目の選択戦略 | 最小相関優先 | パラメータフリー。最も分散投資効果が高い候補から順に追加 |
| ほぼ同一資産の閾値 | 0.98 | corr ≥ 0.98 は実質同一資産であり、追加しても分散効果がない |
| 1 パス目の結果保持 | 必須 | 決定論性と後方互換性を確保（1 パス目で `max_tokens` 個確保できれば 2 パス目は不実行） |

#### 定数の追加

```rust
/// ほぼ同一資産とみなす相関閾値（2パス目の安全弁）
const NEAR_DUPLICATE_THRESHOLD: f64 = 0.98;
```

### 残存する最適性ロスの分析

案 D を適用しても、`apply_constraints()` Phase 1 の MIN_POSITION_SIZE フィルタ
（5% 未満のポジションをゼロ化）による脱落は残る。

#### 影響度の比較

| 制約 | 脱落時の影響 | 案 D での状況 |
|------|-------------|-------------|
| MAX_HOLDINGS | 10% 以上の重みを持つトークンが脱落しうる → normalize で大きな歪み | **解消**（フィルタが no-op） |
| MIN_POSITION_SIZE | 脱落するトークンの重みは < 5% → normalize の歪みは < 5% | 残存するが影響は軽微 |

MAX_HOLDINGS フィルタの脱落では、最適化で 10% 以上の重みが割り当てられたトークンが
上位 6 位に入らず脱落するケースがあり、残存トークンの normalize で 10% 以上の歪みが生じる。
一方、MIN_POSITION_SIZE フィルタの脱落は 5% 未満のトークンのみが対象であり、
normalize の歪みは最大でも各トークンあたり数パーセント以下にとどまる。

現行の MAX_HOLDINGS 脱落と比較して、影響は **一桁小さい**。
入力推定誤差（期待リターン・共分散行列）の範囲内に収まるため、
追加の再最適化は不要と判断する。

#### トークン数減少による正の副次効果

案 D では最適化対象が 6 トークンとなり、均等配分でも各 16.7%（8 トークン時は 12.5%）。
optimizer の重み配分も全体的に底上げされるため、MIN_POSITION_SIZE (5%) を下回る
トークンが発生しにくくなる。これにより MIN_POSITION_SIZE フィルタによる脱落も
間接的に抑制される。

### 実装ステップ

| ステップ | 対象 | 変更内容 |
|----------|------|---------|
| 1 | `execute_portfolio_optimization()` | `MAX_HOLDINGS + 2` → `MAX_HOLDINGS` |
| 2 | `select_uncorrelated_tokens()` | 2 パス方式（最小相関優先フォールバック）の実装 |

> **注意**: Section 5 の MAX_HOLDINGS 設定可能化は案 D のスコープ外とする。
> 案 D は MAX_HOLDINGS の値自体は変更せず、バッファの削減のみを行う。
> 設定可能化は独立したタスクとして別途実施可能。

### テスト戦略

| テスト名 | 目的 |
|----------|------|
| `test_select_uncorrelated_tokens_min_correlation_priority` | 1 パス目で不足時に 2 パス目で最小相関順に追加されること |
| `test_select_uncorrelated_tokens_no_relaxation_needed` | 1 パス目で `max_tokens` 個確保できた場合、2 パス目が不実行であること |
| `test_select_optimal_tokens_max_holdings_no_buffer` | `max_tokens = MAX_HOLDINGS` で MAX_HOLDINGS フィルタが no-op であること |
| `test_select_uncorrelated_tokens_deterministic` | 2 パス方式でも決定論性が保たれること（同一入力 → 同一出力） |
| `test_select_uncorrelated_tokens_near_duplicate_excluded` | corr ≥ 0.98 の候補が 2 パス目でも除外されること |

## 11. 代替アプローチ（最適性重視・外部ソルバ許容）

> 既存の案 B+C / 案 D 以外に、最適性・正当性を優先した場合の代替アプローチを列挙する。
> 本節は「理論上の選択肢」を明確化する目的であり、**採用は前提としない**。

### 代替案の一覧

#### 11.1 MI-SOCP / MIQP（混合整数最適化）

Sharpe 最大化を SOCP（または二次目的）として定式化し、
`MAX_HOLDINGS`（保有数）と `MIN_POSITION_SIZE` を 0-1 変数で表現する。

- **扱える制約**: `max_position`, `MAX_HOLDINGS`, `MIN_POSITION_SIZE` を全て厳密に統合可能
- **最適性**: グローバル最適が理論的に保証される
- **課題**: 外部ソルバ依存、実行時間の不確実性、運用・デプロイの負担

#### 11.2 MIQP の連続緩和 + 丸め

整数制約を連続緩和した問題を解き、上位 K を選ぶなどで丸めを行う。

- **扱える制約**: `max_position` は厳密、`MAX_HOLDINGS` / `MIN_POSITION_SIZE` は近似
- **最適性**: 理論上は緩和解に対する上界/下界を提示できる
- **課題**: 丸めによる最適性ロスが残る

#### 11.3 Reweighted-L1 による疎性誘導 + 反復最適化

`MAX_HOLDINGS` を L0 → reweighted-L1 で近似し、反復でスパース化する。

- **扱える制約**: `max_position` は厳密、`MAX_HOLDINGS` は近似、`MIN_POSITION_SIZE` は閾値処理で併用
- **最適性**: 凸最適化ベースで理論説明が可能（厳密解ではない）
- **課題**: 反復回数と停止性に依存し、局所解に収束する可能性

#### 11.4 IHT / Proximal（Hard Thresholding + 射影）

「最適化 → 上位 K のみ残す → simplex/box への射影」を反復する近似解法。

- **扱える制約**: `MAX_HOLDINGS` は厳密（上位 K 固定）、`max_position` は射影で統合
- **最適性**: 近似解（局所解依存）
- **課題**: 安定性と停止性の理論保証が弱い

#### 11.5 Penalty/Barrier による MIN_POSITION_SIZE の内部化

小さい重みに罰則を付与し、`MIN_POSITION_SIZE` の hard 制約を連続的に近似する。

- **扱える制約**: `max_position` は厳密、`MIN_POSITION_SIZE` は近似、`MAX_HOLDINGS` は別途処理が必要
- **最適性**: 近似解
- **課題**: ペナルティ係数の調整が必要（ハイパーパラメータ依存）

### 比較表（案 B+C / 案 D との相対位置づけ）

| アプローチ | 厳密性 | 停止性/安定性 | 実装コスト | 依存追加 | 推定誤差への頑健性 |
|---|---|---|---|---|---|
| B+C ハイブリッド | 高（box は厳密） | 中（RP は安全弁） | 高 | なし | 中 |
| D トークン選定改善 | 中 | 高 | 低 | なし | 高（誤差支配下で十分） |
| MI-SOCP / MIQP | **最高** | 中（ソルバ依存） | **最高** | **必須** | 低（入力誤差の影響が大きい） |
| 連続緩和 + 丸め | 中 | 中 | 中 | あり | 中 |
| Reweighted-L1 | 中 | 低〜中 | 中 | あり | 中 |
| IHT / Proximal | 低〜中 | 低 | 低 | なし | 中 |
| Penalty/Barrier | 低〜中 | 中 | 低 | なし | 中 |

### 採用しない理由（要約）

1. **外部ソルバ依存の運用コスト**: MI-SOCP/MIQP は正当性が高いが、
   依存追加・実行時間・障害時の運用負担が大きい。
2. **入力推定誤差が支配的**: 厳密解を得ても推定誤差が性能を制限し、
   コストに見合う改善が期待しにくい。
3. **停止性/安定性の懸念**: 近似法は停止性や収束性の保証が弱く、
   実運用での振る舞いが読みにくい。
