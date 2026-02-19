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

## 11. 案 H — 最適性重視の代替アプローチ（外部ソルバ許容）

> Section 3 で却下した案 A（単純再最適化）の発展系として、
> 最適性をより厳密に追求する代替アプローチを列挙する。
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

## 12. 案 G — 2フェーズ最適化アプローチ

> 案 D（トークン選定改善）と案 B+C（内部改修）の間に位置する実用的代替アプローチ。
> 本節は検討候補の列挙であり、**採用判断は行わない**。
> 既存の結論（案 D 採用）は変更しない。

### 12.1 方針概要

全 n トークンで最適化パイプライン（Sharpe + RP + ブレンド）を実行し、
ブレンド後の重み上位 MAX_HOLDINGS トークンを選択、選択したサブセットで再最適化する。

```
execute_portfolio_optimization() — 案 G パイプライン:
  │
  ├─ Phase 1: 全 n トークンで最適化
  │   ├─ w_sharpe = maximize_sharpe_ratio(全体)
  │   ├─ w_rp    = apply_risk_parity(全体)
  │   └─ w_blend = alpha * w_sharpe + (1-alpha) * w_rp
  │
  ├─ Phase 2: トークン選択（optimizer-informed）
  │   └─ survivors = w_blend の重み上位 MAX_HOLDINGS 個
  │       （n <= MAX_HOLDINGS なら Phase 3 をスキップ）
  │
  ├─ Phase 3: サブセットで再最適化
  │   ├─ (sub_returns, sub_cov) = extract_sub_portfolio(survivors)
  │   ├─ w_sharpe_sub = maximize_sharpe_ratio(sub)
  │   ├─ w_rp_sub = apply_risk_parity(sub)
  │   ├─ w_blend_sub = alpha * w_sharpe_sub + (1-alpha) * w_rp_sub
  │   └─ candidate = embed_to_full(n, survivors, w_blend_sub)
  │
  ├─ Phase 4: 両方に制約適用
  │   ├─ apply_constraints(&mut original,  max_position)   // Phase 1 結果
  │   └─ apply_constraints(&mut candidate, max_position)   // Phase 3 結果
  │
  └─ Phase 5: セーフガード
        sharpe_candidate >= sharpe_original → candidate 採用
        otherwise → original 維持（既存動作と同一）
```

### 12.2 案 B の reoptimize_for_survivors() との差異

案 B（Section 6）は `apply_constraints()` 適用**後**の生存トークンで再最適化するが、
案 G は `apply_constraints()` 適用**前**の optimizer 重みでトークンを選択する。

| 観点 | 案 B (reoptimize_for_survivors) | 案 G (2フェーズ) |
|------|--------------------------------|-----------------|
| トークン選択タイミング | 制約適用後（box clamp → MAX_HOLDINGS → MIN_POSITION_SIZE 後） | 制約適用前（optimizer の生の重み順） |
| 選択基準 | 制約で歪んだ重みの非ゼロ判定 | optimizer の共分散判断に基づく重み順 |
| box 制約の影響 | box clamp が重み順を変える可能性あり | 影響なし（生の重みで判断） |
| maximize_sharpe_ratio 変更 | 必要（案 C: シグネチャ変更 + box 制約統合） | **不要**（既存関数をそのまま使用） |
| apply_risk_parity 変更 | 必要（案 C: シグネチャ変更 + 固定集合法） | **不要** |
| apply_constraints 変更 | 必要（Phase 1 から box clamp 除去） | **不要** |

案 G の最大の利点は **既存の optimizer 関数を一切変更しない** こと。
新設するのは `extract_sub_portfolio()` と `execute_portfolio_optimization()` 内の
2フェーズロジックのみ。

### 12.3 Section 11.4 IHT との関係

Section 11.4 の IHT（Iterative Hard Thresholding）は
「最適化 → 上位 K のみ残す → 射影 → **反復**」するアプローチ。
案 G はこの**単一パス版**（反復なし）に相当する。

- IHT の停止性懸念（反復の振動リスク）を完全に回避
- 単一パスでも n-k=2 の小規模問題では十分な近似
- セーフガードにより既存動作の悪化を防止

### 12.4 案 D との比較

| 観点 | 案 D | 案 G |
|------|------|------|
| 実装ステップ | 2 | 3（extract_sub_portfolio + 2フェーズ + セーフガード） |
| 新設関数 | 0 | 1（extract_sub_portfolio） |
| 既存関数改修 | 2（select_uncorrelated_tokens + 定数変更） | 1（execute_portfolio_optimization） |
| maximize_sharpe_ratio 変更 | 不要 | 不要 |
| apply_risk_parity 変更 | 不要 | 不要 |
| apply_constraints 変更 | 不要 | 不要 |
| 最適化実行回数 | 1 | 2 |
| トークン選択の根拠 | ヒューリスティックスコア + 相関フィルタ | optimizer の共分散判断 |
| D の弱点対応 | なし | 直接解決 |
| セーフガード | 不要（脱落が発生しない） | 推奨（再最適化の安全弁） |
| 過学習リスク | なし | 低（2候補の比較のみ） |

### 12.5 案 D の弱点が問題になるケース

案 D は `select_uncorrelated_tokens` の貪欲法（スコア降順 + 相関フィルタ）で
トークンを選定する。この選定と optimizer の重み順が乖離するのは:

1. **スコアは低いが分散投資効果が大きいトークン**: 複合スコア（Sharpe 40% + 流動性 20% +
   信頼度 20% + vol rank 20%）が低くても、他トークンとの低相関により
   ポートフォリオ全体のリスクを大幅に下げるケース
2. **相関フィルタで除外されたが有用なトークン**: 閾値 0.85 で除外されたトークンが、
   optimizer の共分散行列全体の考慮では有用と判断されるケース

ただし:
- 相関フィルタが部分的に共分散構造をカバーしている
- n=8→6 の差は2トークンのみで影響範囲は限定的
- 入力推定誤差が支配的

### 12.6 セーフガード設計

案 B（Section 8）のセーフガードと同一ロジックを再利用:

```
セーフガード判定:
  sharpe_original >= 0 または sharpe_candidate >= 0:
    sharpe_candidate >= sharpe_original → 採用
  両方 < 0:
    candidate_return > original_return かつ
    candidate_std ≤ original_std × 1.1 → 採用
  otherwise: 元の重みを維持
```

### 12.7 実装ステップ

| ステップ | 対象 | 変更内容 |
|----------|------|---------|
| 1 | `extract_sub_portfolio()` 新設 | サブ期待リターン + サブ共分散行列の抽出 |
| 2 | `execute_portfolio_optimization()` 改修 | 2フェーズ最適化 + セーフガードの統合 |
| 3 | テスト追加 | 下記テスト戦略参照 |

### 12.8 テスト戦略

| テスト名 | 目的 |
|----------|------|
| `test_extract_sub_portfolio` | サブ問題の抽出が正しいか |
| `test_two_phase_preserves_or_improves_sharpe` | 2フェーズ後の Sharpe が元以上であること |
| `test_two_phase_safeguard_reverts` | セーフガード発動時に元の重みが維持されること |
| `test_two_phase_noop_when_n_leq_max_holdings` | n ≤ MAX_HOLDINGS で Phase 3 がスキップされること |
| `test_two_phase_satisfies_all_constraints` | 再最適化後も全制約を満たすこと |

### 12.9 案 D との関係

案 G は案 D の**代替**に位置する（組合せは不可）:

- **案 D の代替として使う場合**: `max_tokens = MAX_HOLDINGS + 2` を維持し、
  2フェーズ最適化で最適なサブセットを自動選択。select_uncorrelated_tokens の
  2パス改修は不要。
- **案 D との組合せ**: `max_tokens = MAX_HOLDINGS` にすると n ≤ MAX_HOLDINGS で
  Phase 3 がスキップされ実質 no-op になるため、組合せの意味がない。

案 D は「脱落自体を防ぐ」アプローチ、案 G は「脱落させてから最適に回復する」アプローチ。

## 13. 案 D/G/H 比較検討 — Optimizer 信頼性評価を含む

> 案 D（Section 10）、案 G（Section 12）、案 H（Section 11 の MI-SOCP/MIQP）を
> optimizer への入力推定品質の観点から再評価し、採用判断の根拠を補強する。
> 既存の結論（案 D 採用）は変更しない。

### 13.1 Optimizer 入力の推定品質

現行実装の調査結果に基づき、optimizer への入力（μ, Σ）の推定品質を評価する。

**期待リターン μ**（`portfolio.rs:113-133`）:

- 24h 価格予測モデルの単点推定。ヒストリカルリターンとのブレンドなし
- 予測精度に直接依存。予測信頼度は alpha ブレンドのみに使用（optimizer 自体は未使用）
- 推定誤差: **非常に大きい**（モデル精度依存）

**共分散行列 Σ**（`portfolio.rs:150-184`）:

- 30 日サンプル共分散 + 固定正則化（1e-6）+ PSD 保証（eigenvalue clamp）
- シュリンケージ推定量（Ledoit-Wolf 等）未使用
- 8 トークンで 36 自由パラメータに対しデータ点 29（自由度不足気味）
- 推定誤差: **中〜大**

**ロバストネステスト結果**（`tests.rs:1351-1377`）:

- ±5% リターンノイズで重み変化 < 20%
- 現実的な推定誤差（±30-50%）は未テスト

### 13.2 Optimizer の「トークン選択者」としての信頼性

optimizer の重み順がヒューリスティックスコア順より信頼できるかの評価:

| 評価軸 | 信頼性 | 理由 |
|--------|--------|------|
| 数値的安定性 | 高 | PSD 保証 + Cholesky/LU フォールバック |
| 重み「値」の精度 | 低 | μ の推定誤差が Σ⁻¹ で増幅 |
| 重み「順位」の安定性 | 中 | 上位は安定、境界付近（6位 vs 7位）は不安定 |
| 複合スコア対比の追加情報量 | 限定的 | 多変量共分散構造は追加情報だが、流動性・信頼度は欠落 |

### 13.3 複合スコアと optimizer の情報比較

案 D の複合スコア（Section 10）と案 G の optimizer（Section 12）が
それぞれ利用する情報源を比較する。

| 情報源 | 複合スコア (案 D) | Optimizer (案 G) |
|--------|-------------------|------------------|
| 期待リターン μ | Sharpe 成分（40%） | Σ⁻¹μ_excess で直接使用 |
| ボラティリティ σ | Sharpe 成分 + vol_rank（20%） | Σ の対角要素 |
| 共分散構造（多変量） | ペアワイズ相関のみ | 全体を使用（Σ⁻¹） |
| 流動性 | 20% の重み | 未使用 |
| 予測信頼度 | 20% の重み | 未使用（alpha ブレンドのみ） |

重要な観察: 複合スコアは optimizer にない実用的情報（流動性・信頼度）を含む。
一方、optimizer は多変量共分散構造を完全に活用する。
ただし Section 13.1 で述べた通り、Σ の推定品質が中〜大の誤差を含むため、
この理論的優位性は減殺される。

### 13.4 案 D/G/H 総合比較

Optimizer 信頼性を踏まえた各案の評価:

| 判断基準 | 案 D | 案 G | 案 H |
|---|---|---|---|
| 実装コスト | 低（~50行） | 中（~100行） | 高（数百行+外部依存） |
| 選択の理論的質 | 中（ヒューリスティック） | 中〜高（μ誤差で減殺） | 高（厳密、ただし非現実的） |
| 安全性 | 暗黙的（セーフガードなし） | 明示的（Sharpe セーフガード） | — |
| 推定誤差下の頑健性 | 高（流動性・信頼度含む） | 中（セーフガードで補償） | 低（過剰仕様） |

- **案 D vs 案 G**: 案 G は共分散構造の完全活用とセーフガードの安全性で理論的に優れるが、
  境界付近のトークン選択（6位 vs 7位）の信頼性が低い現状では、
  流動性・信頼度を含む複合スコアの方が実用的に頑健
- **案 G vs 案 H**: 案 G は案 H の「入力誤差が支配的」問題（Section 11 参照）を
  セーフガードで緩和するが、根本的な制約（μ の低精度）は共有

### 13.5 結論

- **案 H**: 不採用。外部ソルバ依存のコスト・リスクが、
  入力推定誤差が支配的な状況での改善幅に見合わない（Section 11 の結論と整合）
- **案 G**: 理論的優位性は中程度。セーフガードの安全性は価値があるが、
  optimizer の境界判断（6位 vs 7位）の信頼性が低い現状では、
  ヒューリスティックに対する明確な優位は限定的
- **案 D**: 現状の入力品質（μ 低精度、Σ 中精度）では実用的に合理的。
  推定精度の改善（シュリンケージ共分散、ベイズ縮小リターン等）が先行すれば
  案 G の価値が上がるが、入力改善は本改修のスコープ外

既存結論（Section 10: 案 D 採用）を維持する。

## 14. Optimizer 改善アプローチカタログ

> Section 13.5 で「入力改善は本改修のスコープ外」と結論した。
> 本セクションはそのスコープ外領域を体系的に整理し、将来の改善ロードマップを提示する。
> 4 カテゴリ × 16 アプローチを列挙し、費用対効果に基づき Phase 1〜4 + 非推奨に分類する。

### 14.1 改善カテゴリ概要

| カテゴリ | 対象 | アプローチ数 | 主な課題 |
|----------|------|:---:|----------|
| A. 入力品質: μ（期待リターン） | `calculate_expected_returns()` | 4 | 単点推定の低精度（Section 13.1） |
| B. 入力品質: Σ（共分散行列） | `calculate_covariance_matrix()` | 4 | n/T 比 0.28 の自由度不足（Section 13.1） |
| C. 最適化アルゴリズム | `maximize_sharpe_ratio()` 等 | 5 | 推定誤差の増幅抑制 |
| D. パイプライン / アーキテクチャ | 全体構成 | 3 | 効果測定・適応的制御 |

各アプローチには以下を記載する:

- **概要**: 手法の説明
- **期待効果**: 改善が見込まれる点
- **実装コスト**: 行数目安と工数感
- **外部依存**: 追加クレート等の有無

### 14.2 入力品質: μ（期待リターン）

現行の μ 計算（`portfolio.rs:113-133`）は Chronos の 24h 単点予測に完全依存しており、
ヒストリカルリターンとのブレンドや信頼区間の活用がない。
予測信頼度は alpha ブレンド（`portfolio.rs:930-944`）のみに使用され、
optimizer 自体には渡されていない。

#### 1-C Winsorization（Phase 1, ~30 行）

| 項目 | 内容 |
|------|------|
| 概要 | μ を一定範囲（例: ±3σ）にクランプし、外れ値的な予測を抑制する |
| 期待効果 | 極端な予測による重み集中を防止。Σ⁻¹μ の暴走を直接抑制 |
| 実装コスト | ~30 行。`calculate_expected_returns()` 内に clamp ロジック追加 |
| 外部依存 | なし |

最小コストで即効性があり、他の全アプローチと併用可能。

#### 1-B ヒストリカルリターンブレンド（Phase 2, ~70 行）

| 項目 | 内容 |
|------|------|
| 概要 | 予測リターンと過去 N 日の平均リターンの加重平均を μ として使用。Black-Litterman の簡易版 |
| 期待効果 | 予測精度が低い場合にヒストリカルデータがアンカーとして機能し、μ の安定性向上 |
| 実装コスト | ~70 行。日次リターン（既に `calculate_daily_returns()` で計算済み）の平均を算出しブレンド |
| 外部依存 | なし |

ブレンド比率の決定が課題。固定比率（例: 予測 60% + 実績 40%）から開始し、
後述の 4-D（信頼区間活用）で動的調整に発展可能。

#### 4-D Chronos 信頼区間活用（Phase 2, ~100 行）

| 項目 | 内容 |
|------|------|
| 概要 | 既存の `ChronosPredictionResponse` に含まれる `lower_bound`（10%ile）/ `upper_bound`（90%ile）（`prediction.rs:12-15`）を活用し、予測の不確実性を定量化 |
| 期待効果 | 信頼区間幅から銘柄ごとの予測精度を推定。1-B のブレンド比率や Black-Litterman の Ω 行列のデータソースとして使用可能 |
| 実装コスト | ~100 行。信頼区間の取得は既存インフラで完了済み、optimizer への伝搬パスの構築が主な作業 |
| 外部依存 | なし（既存の Chronos API レスポンスに含まれる） |

現在完全に未活用のデータを活用する点で、投資対効果が高い。

#### 1-A Black-Litterman（Phase 3, ~250 行）

| 項目 | 内容 |
|------|------|
| 概要 | 均衡リターン（市場ポートフォリオからの逆算）を事前分布、Chronos 予測をビューとしてベイズ結合。μ_BL = [(τΣ)⁻¹ + P'Ω⁻¹P]⁻¹ [(τΣ)⁻¹π + P'Ω⁻¹Q] |
| 期待効果 | 予測精度が低くても均衡リターンがアンカーとなり、μ の安定性が大幅に向上。理論的に最も洗練されたアプローチ |
| 実装コスト | ~250 行。均衡リターン π の定義（DEX 市場に適した基準の設計）、4-D の信頼区間から Ω 行列の構築が必要 |
| 外部依存 | なし（行列演算は nalgebra で実装可能） |

Phase 2 の 4-D（信頼区間活用）が前提条件。DEX 環境での「均衡リターン」の定義が設計上の主要課題。

### 14.3 入力品質: Σ（共分散行列）

現行の Σ 計算（`portfolio.rs:150-184`）は 30 日サンプル共分散 + 固定正則化（`REGULARIZATION_FACTOR = 1e-6`、`portfolio.rs:65`）+
PSD 保証（`ensure_positive_semi_definite()`、`portfolio.rs:190-227`）で構成される。
8 トークン（36 自由パラメータ）に対しデータ点 29 で、自由度が不足気味。

#### 2-A Ledoit-Wolf 縮小推定（Phase 1, ~200 行）

| 項目 | 内容 |
|------|------|
| 概要 | サンプル共分散行列を構造化ターゲット（単位行列のスケーリング等）へ最適縮小。Σ_LW = δ·F + (1-δ)·S（F: ターゲット、S: サンプル共分散、δ: 最適縮小係数） |
| 期待効果 | n/T 比 (8/29 ≈ 0.28) の自由度不足問題を直接解決。推定誤差を理論的に最小化（MSE 最小の線形縮小） |
| 実装コスト | ~200 行。解析的に最適縮小係数 δ を計算する Ledoit-Wolf (2004) アルゴリズムの実装 |
| 外部依存 | なし（ndarray + nalgebra で実装可能） |

現行の固定正則化（`portfolio.rs:65,177`）をデータ駆動の縮小推定に置換する。
Phase 1 で最も効果が大きいアプローチ。

#### 2-B EWMA 共分散（Phase 2, ~100 行）

| 項目 | 内容 |
|------|------|
| 概要 | 指数加重移動平均（Exponentially Weighted Moving Average）で共分散を計算。直近のデータに大きな重みを付与 |
| 期待効果 | レジームチェンジ（市場環境の急変）への追従性改善。等重みサンプル共分散の古いデータの影響を低減 |
| 実装コスト | ~100 行。`calculate_covariance_matrix()` の重み付け計算への変更 |
| 外部依存 | なし |

2-A（Ledoit-Wolf）と併用可能: EWMA 共分散を計算後、Ledoit-Wolf で縮小推定。

#### 2-C ファクターモデル（非推奨）

| 項目 | 内容 |
|------|------|
| 概要 | 共通ファクター（市場リスク等）で共分散構造をモデル化し、推定パラメータ数を削減 |
| 不採用理由 | n=8 の小規模問題では Ledoit-Wolf で十分なパラメータ削減効果が得られる。ファクターの定義・推定コストが不釣り合い |

#### 2-D DCC-GARCH（非推奨）

| 項目 | 内容 |
|------|------|
| 概要 | 動的条件付き相関 GARCH モデルで時変共分散を推定 |
| 不採用理由 | 実装コストが最大（外部クレート or 数百行の自前実装）。EWMA（2-B）で十分な動的対応が可能 |

### 14.4 最適化アルゴリズム

現行の最適化は Sharpe 最大化（解析解 + アクティブセット法、`portfolio.rs:270-362`）と
Risk Parity（`portfolio.rs:364-419`）の alpha ブレンド（`portfolio.rs:930-944`）で構成される。

#### 3-C L2 正則化（Phase 1, ~40 行）

| 項目 | 内容 |
|------|------|
| 概要 | 目的関数に L2 ペナルティ ‖w‖² を追加、または等価的に Σ + λI で共分散行列を正則化。等配分への暗黙的縮小効果 |
| 期待効果 | 推定誤差による極端な重み集中を抑制。Ledoit-Wolf（2-A）と補完的に機能 |
| 実装コスト | ~40 行。`maximize_sharpe_ratio()` 内の共分散行列に λI を加算（現行の `REGULARIZATION_FACTOR` の拡張） |
| 外部依存 | なし |

現行の `REGULARIZATION_FACTOR = 1e-6`（数値安定性目的）を、推定誤差抑制を意図した
より大きな値（例: 1e-3〜1e-2）に調整する形で実装可能。

#### 3-B Resampled Efficient Frontier — Michaud（Phase 4, ~180 行）

| 項目 | 内容 |
|------|------|
| 概要 | μ, Σ からモンテカルロサンプリング → 各サンプルで最適化 → 重み平均。推定誤差を直接考慮 |
| 期待効果 | 推定誤差のある入力に対してロバストな重みを生成。バックテストでの改善が報告されている |
| 実装コスト | ~180 行。MC サンプル生成 + 既存 `maximize_sharpe_ratio()` の複数回呼び出し + 重み平均 |
| 外部依存 | `rand` クレート（乱数生成） |

Phase 1〜2 の入力改善が先行すべき。入力品質が改善された後に価値が上がる。

#### 3-A ロバスト最適化（Phase 4, ~350 行）

| 項目 | 内容 |
|------|------|
| 概要 | μ, Σ の不確実性集合を定義し、worst-case シナリオを最適化。min-max 問題として定式化 |
| 期待効果 | 入力推定誤差に対する最悪ケースの制御。Black-Litterman 未実装の場合に価値が大きい |
| 実装コスト | ~350 行。不確実性集合の設計 + 二次錐計画（SOCP）ソルバ or 反復法の実装 |
| 外部依存 | SOCP ソルバ（外部クレート）が望ましいが、近似解法なら自前実装可能 |

Black-Litterman（1-A）が実装されれば必要性は大幅に低下する。

#### 3-D 最小分散フォールバック（Phase 4, ~70 行）

| 項目 | 内容 |
|------|------|
| 概要 | μ を完全に無視し、Σ のみで最小分散ポートフォリオを計算。μ 推定が信頼できない場合のフォールバック |
| 期待効果 | μ 推定誤差の影響を完全に排除 |
| 実装コスト | ~70 行。`maximize_sharpe_ratio()` から μ 依存を除去した変種 |
| 外部依存 | なし |

現行の Risk Parity（`portfolio.rs:364-419`）が類似の役割を果たしているため、追加価値は限定的。

#### 3-E CVaR 最適化（非推奨）

| 項目 | 内容 |
|------|------|
| 概要 | 条件付き VaR（Conditional Value-at-Risk）を最小化。テールリスクの明示的制御 |
| 不採用理由 | LP ソルバ依存。n=8 の小規模問題では mean-variance + Risk Parity ブレンドで十分なリスク管理が可能 |

### 14.5 パイプライン / アーキテクチャ

#### 4-C バックテスト（Phase 3, ~500 行）

| 項目 | 内容 |
|------|------|
| 概要 | ウォークフォワードテスト基盤の構築。過去データで各改善アプローチの効果を定量測定 |
| 期待効果 | 改善効果の客観的評価基盤。「どの改善がどれだけ Sharpe を改善したか」を定量的に回答可能にする |
| 実装コスト | ~500 行。データローダ + ウォークフォワードループ + メトリクス集計 |
| 外部依存 | なし（既存の DB データを利用） |

全改善の効果測定基盤として Phase 3 に配置。Phase 1〜2 の改善を適用した状態と未適用状態の比較が可能。

#### 4-A 適応的ブレンド（Phase 4, ~120 行）

| 項目 | 内容 |
|------|------|
| 概要 | 共分散行列の条件数・サンプル充足度（n/T 比）・予測信頼度に基づき、alpha（Sharpe vs RP のブレンド比率）を動的調整 |
| 期待効果 | 入力品質が低い状況で自動的に RP 寄りにシフトし、ロバスト性を確保 |
| 実装コスト | ~120 行。条件数計算 + alpha 調整ロジック（現行の `volatility_blend_alpha()` + `prediction_confidence` 調整の拡張） |
| 外部依存 | なし |

Black-Litterman（1-A）未実装の場合に価値が大きい。BL 実装後は優先度が下がる。

#### 4-B マルチホライゾン統合（Phase 4, ~180 行）

| 項目 | 内容 |
|------|------|
| 概要 | 複数時間軸（例: 6h, 12h, 24h, 48h）の予測を統合し、μ の安定性を向上 |
| 期待効果 | 単一ホライゾンの予測ノイズを平滑化 |
| 実装コスト | ~180 行。複数ホライゾン予測の取得 + 加重平均ロジック |
| 外部依存 | Chronos API の複数ホライゾン対応が前提 |

#### 1-D アンサンブル予測（Phase 4, ~150 行）

| 項目 | 内容 |
|------|------|
| 概要 | 複数予測モデル（Chronos + 移動平均 + ARIMA 等）の加重平均で μ を推定 |
| 期待効果 | 単一モデルへの依存リスク低減。予測の分散を低減（アンサンブル効果） |
| 実装コスト | ~150 行。追加予測モデル実装 + アンサンブルロジック |
| 外部依存 | 追加予測モデルの選定・実装 |

### 14.6 推奨フェーズ順序

| Phase | アプローチ | 合計行数目安 | 対象ファイル | 根拠 |
|:---:|---|:---:|---|---|
| **1** | 1-C Winsorization (~30), 2-A Ledoit-Wolf (~200), 3-C L2 正則化 (~40) | ~270 | `portfolio.rs` 内で完結 | 即効・低コスト、相互独立で並行実装可能、外部依存なし |
| **2** | 4-D Chronos 信頼区間 (~100), 1-B ヒストリカルブレンド (~70), 2-B EWMA 共分散 (~100) | ~270 | `portfolio.rs` + 予測パイプライン | μ の構造的改善。既存の未活用データ（信頼区間）を活用 |
| **3** | 1-A Black-Litterman (~250), 4-C バックテスト (~500) | ~750 | `portfolio.rs` + 新モジュール | 理論的に最も洗練。Phase 2 の 4-D 完了が BL の前提条件 |
| **4** | 3-B Michaud, 3-A ロバスト, 4-A 適応ブレンド, 4-B マルチホライゾン, 1-D アンサンブル | — | — | 状況次第。BL 実装後は 3-A, 4-A の優先度低下 |
| 非推奨 | 2-C ファクターモデル, 2-D DCC-GARCH, 3-E CVaR | — | — | n=8 では費用対効果が不釣り合い |

**Phase 1 の詳細**:

- 3 アプローチとも相互独立のため並行実装可能
- すべて `portfolio.rs` 内で完結し、外部クレート追加不要
- 既存テスト（ロバストネステスト等）で効果を即座に確認可能

**Phase 2 の詳細**:

- 4-D（信頼区間活用）が 1-B（ブレンド比率の動的調整）と 1-A（BL の Ω 行列）の基盤
- 2-B（EWMA）は 2-A（Ledoit-Wolf）と直列に適用可能

**Phase 3 の詳細**:

- Black-Litterman は Phase 2 の 4-D 完了が前提条件（Ω 行列のデータソース）
- バックテストは Phase 1〜2 の改善効果を定量評価する基盤

### 14.7 既存結論との関係

- **Section 13.5 との関係**: 「入力改善は本改修のスコープ外」を受けた将来改善の見取図。
  本セクションは当面の実装対象ではなく、案 D 採用後の次フェーズとして位置づける

- **案 G（Section 12）の再評価**: Phase 1 完了（特に Ledoit-Wolf による Σ 推定改善 +
  Winsorization による μ クランプ）後に、optimizer のトークン選択信頼性（Section 13.2）が
  向上する。これにより案 G の「optimizer をトークン選択者として使う」価値が上がり、
  再評価の対象となる

- **Chronos 信頼区間の未活用**: `ChronosPredictionResponse`（`prediction.rs:9-20`）に
  `lower_bound`（10%ile）/ `upper_bound`（90%ile）が既に存在するが、
  現行実装では完全に未活用。Phase 2 の 4-D でこれを活用することが、
  μ 改善の鍵となる低コスト施策

- **Section 10（案 D）への影響**: 本カタログの改善はいずれも案 D と独立に適用可能。
  案 D のトークン選定と、optimizer への入力改善は直交する改善軸である
