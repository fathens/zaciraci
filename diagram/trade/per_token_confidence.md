# Auto Trade 改善計画: Per-token Confidence + 低 Confidence トークン除外

## Context

直近10日間で 19.91 → 14.06 NEAR（-29%）。Buy-and-hold なら +1.1%。
30%の差は毎日のトークンローテーション（売買チャーン）による。

### 根本原因チェーン

```
fm-1（MAPE 22,733%、非取引対象）が全体の rolling MAPE を汚染
  → confidence = 0.000
  → alpha = 0.5（Sharpe 50% + RP 50%）
  → 全トークンに同じ不安定な alpha が適用
  → 日次でウエイトが大きく変動
  → 毎日リバランストリガー → チャーンコスト蓄積
```

### 改善方針: トークンごとの予測精度評価 + 低 confidence トークンの除外

グローバルな confidence（全トークン平均）ではなく、**個々のトークンの予測精度で評価**する。
**予測が信頼できないトークンはトークン選定時に除外**し、投資対象にしない。

- blackdragon（MAPE 2.3%）→ 高信頼 → 投資対象、Sharpe ウエイトを信頼
- ftv2（MAPE 1.8%）→ 高信頼 → 投資対象、Sharpe ウエイトを信頼
- nearkat（MAPE 25.6%）→ 低信頼 → **除外**（予測でエッジがないなら投資しない）
- fm-1（MAPE 22,733%）→ 信頼ゼロ → **除外**

**「予測が安定している」だけでは購入理由にならない。予測で資産を増やせる必要がある。**
低 confidence トークンは RP フォールバック（少額配分）ではなく、完全除外する。

---

## 改善案 0: is_new_period 判定バグ修正

### 対象ファイル
- `crates/trade/src/execution.rs:750`

### 実装
```rust
// 変更前
let is_new_period = transaction_count == 0;
// 変更後: 両条件の組み合わせ
// selected_tokens.is_empty() だけだとパース全失敗（データ破損）時に誤判定するため、
// transaction_count == 0 も併用して安全性を確保
let is_new_period = selected_tokens.is_empty() && transaction_count == 0;
```

---

## 改善案 1: トークンごとの prediction confidence + 低 confidence トークン除外（最重要）

### 概要

2段階の改善:
1. **トークン選定時フィルタ**: confidence < 閾値のトークンを除外（投資対象にしない）
2. **per-token alpha**: 残ったトークン内で confidence に応じた Sharpe/RP ブレンド

`PortfolioData.prediction_confidence: Option<f64>`（単一スカラー）を
`BTreeMap<TokenOutAccount, f64>`（トークンごと）に変更。

### 対象ファイル
- `crates/common/src/algorithm/portfolio.rs` — per-token alpha
- `crates/common/src/algorithm/portfolio.rs` — `PortfolioData` 構造体
- `crates/common/src/config/typed.rs` — TRADE_MIN_TOKEN_CONFIDENCE 設定追加
- `crates/trade/src/strategy.rs` — per-token confidence の計算、フィルタ、受け渡し
- `crates/trade/src/prediction_accuracy.rs` — per-token confidence 計算関数

### 1-0. confidence フィルタの位置（execute_portfolio_strategy() 内、予測後）

**重要な設計判断**: フィルタはトークン選定時（start()）ではなく、
`execute_portfolio_strategy()` 内の予測実行後に配置する。

理由: 予測を全トークンで実行し続けることで、除外トークンの MAPE が更新され続ける。
予測精度が回復すれば自然にポートフォリオに復帰する。
予測を止めると MAPE が凍結され、永久除外（デッドロック）になる。

```
predict_multiple_tokens(全10トークン)  ← 予測コスト変更なし（現状と同じ）
    ↓
record_predictions(全10トークン)        ← MAPE が全トークンで更新される
    ↓
calculate_per_token_confidence()        ← per-token confidence 計算
    ↓
[NEW] confidence フィルタ: 低 confidence トークンを token_data / predictions から除外
    ↓
PortfolioData 構築（高 confidence トークンのみ）
    ↓
execute_portfolio_optimization()
```

```rust
// config に追加（typed.rs）
/// Minimum per-token prediction confidence to include in portfolio.
/// Tokens below this threshold are excluded from trading.
fn trade_min_token_confidence() -> f64 {
    key: "TRADE_MIN_TOKEN_CONFIDENCE",
    default: 0.3
}
```

デフォルト 0.3 の根拠:
- confidence = 0.6 * mape_confidence + 0.4 * direction_confidence
- 0.3 ≈ MAPE ~11% かつ方向ヒット率 ~50%
- これ以下は予測にエッジがなく、投資しても期待値がゼロ以下

**挿入位置**: `execute_portfolio_strategy()` 内、token_data 集約（L571-611）の後、
PortfolioData 構築の前。

```rust
// token_data 集約完了後（record_predictions() 実行後）
// per-token confidence を計算
let token_out_list: Vec<TokenOutAccount> = token_data.iter()
    .map(|t| t.symbol.clone())
    .collect();
let prediction_confidences = super::prediction_accuracy::calculate_per_token_confidence(
    &token_out_list, cfg
).await;

// 低 confidence トークンを除外（予測は既に実行済み → MAPE は更新される）
let min_confidence = cfg.trade_min_token_confidence();
let original_count = token_data.len();
token_data.retain(|t| {
    let confidence = prediction_confidences.get(&t.symbol).copied();
    match confidence {
        Some(c) if c < min_confidence => {
            info!(log, "excluding token due to low confidence";
                "token" => %t.symbol, "confidence" => format!("{:.3}", c));
            false
        }
        None => true,  // データなし（コールドスタート）は除外しない
        _ => true,
    }
});
// predictions と historical_prices も同期して除外
predictions.retain(|k, _| token_data.iter().any(|t| &t.symbol == k));
historical_prices.retain(|k, _| token_data.iter().any(|t| &t.symbol == k));

if token_data.len() < original_count {
    info!(log, "tokens filtered by prediction confidence";
        "original" => original_count, "remaining" => token_data.len());
}

// 全トークン除外のエッジケース: 安全に Hold を返す
// 全トークンが閾値未満 = 予測モデル全体が信頼できない状態。
// ポジションを取らず Hold で安全に停止する。
if token_data.is_empty() {
    warn!(log, "all tokens below confidence threshold, holding";
        "threshold" => format!("{:.3}", min_confidence));
    return Ok(vec![TradingAction::Hold]);
}
```

### 1-1. PortfolioData の変更（portfolio.rs）

```rust
// 変更前
pub struct PortfolioData {
    pub tokens: Vec<TokenData>,
    pub predictions: BTreeMap<TokenOutAccount, TokenPrice>,
    pub historical_prices: BTreeMap<TokenOutAccount, PriceHistory>,
    pub prediction_confidence: Option<f64>,  // 単一スカラー
}

// 変更後
pub struct PortfolioData {
    pub tokens: Vec<TokenData>,
    pub predictions: BTreeMap<TokenOutAccount, TokenPrice>,
    pub historical_prices: BTreeMap<TokenOutAccount, PriceHistory>,
    pub prediction_confidences: BTreeMap<TokenOutAccount, f64>,  // トークンごと
}
```

### 1-2. execute_portfolio_optimization() の変更（portfolio.rs:1583-1590）

```rust
// 変更前: 単一 alpha
let alpha = match portfolio_data.prediction_confidence {
    Some(confidence) => {
        let floor = PREDICTION_ALPHA_FLOOR;
        (floor + (alpha_vol - floor) * confidence).clamp(floor, 0.9)
    }
    None => alpha_vol,
};

// 変更後: トークンごとの alpha 配列
let alphas: Vec<f64> = selected_tokens.iter()
    .map(|t| {
        let confidence = portfolio_data.prediction_confidences
            .get(&t.symbol)
            .copied();
        // データなし（コールドスタート）→ alpha_vol * 0.5 にフォールバック
        // alpha_vol そのままだと「予測精度未評価」と「予測精度が高い」が同等になる
        // 0.0 だと新規トークンが全て RP になり保守的すぎる
        let confidence = match confidence {
            Some(c) => c,
            None => return alpha_vol * 0.5, // 控えめなデフォルト
        };
        let floor = PREDICTION_ALPHA_FLOOR;
        (floor + (alpha_vol - floor) * confidence).clamp(floor, 0.9)
    })
    .collect();
```

### 1-3. unified_optimize() の変更（portfolio.rs:1319）

```rust
// 変更前
fn unified_optimize(
    ...,
    alpha: f64,
) -> Vec<f64>

// 変更後
fn unified_optimize(
    ...,
    alphas: &[f64],  // トークンごとの alpha
) -> Vec<f64>
```

Phase 2 枝刈り（L1374）— per-token alpha ブレンド:
```rust
// 変更前
let blend = alpha * w_sharpe[i] + (1.0 - alpha) * w_rp[i];
// 変更後: per-token alpha でブレンド（Phase 3 以降と一貫）
let blend = alphas[i] * w_sharpe[i] + (1.0 - alphas[i]) * w_rp[i];
```

### 1-4. SubsetOptParams / blend_and_expand() の変更

```rust
// 変更前
struct SubsetOptParams<'a> {
    expected_returns: &'a [f64],
    covariance_matrix: &'a Array2<f64>,
    max_position: f64,
    alpha: f64,            // 単一
}

// 変更後
struct SubsetOptParams<'a> {
    expected_returns: &'a [f64],
    covariance_matrix: &'a Array2<f64>,
    max_position: f64,
    alphas: &'a [f64],    // トークンごと
}
```

`blend_and_expand()`（L955-978）— シグネチャに `alphas` を追加:
```rust
// 変更後
fn blend_and_expand(
    sub_returns: &[f64], sub_cov: &Array2<f64>,
    max_position: f64, alphas: &[f64],  // フルサイズ配列（n_total 長）
    subset_indices: &[usize], n_total: usize,
) -> Vec<f64> {
    assert_eq!(alphas.len(), n_total,
        "alphas length ({}) must equal n_total ({})", alphas.len(), n_total);
    // ...
}
```

ブレンド計算:
```rust
// 変更前
.map(|(&ws, &wr)| alpha * ws + (1.0 - alpha) * wr)

// 変更後
.enumerate()
.map(|(i, (&ws, &wr))| {
    let a = alphas[subset_indices[i]];
    a * ws + (1.0 - a) * wr
})
```

`cached_blend_and_expand()` も同様に `params.alphas` を `blend_and_expand()` にリレー。

`exhaustive_optimize()` 内のスコア計算（L1295）:
```rust
// 変更前
let score = alpha * sharpe - (1.0 - alpha) * rp_div_normalized;

// 変更後: アクティブトークンの alpha 単純平均を使用
// 注: 加重平均はウエイト→alpha→ウエイトの循環依存になるため不可
let effective_alpha: f64 = active_idx.iter()
    .map(|&idx| params.alphas[idx])
    .sum::<f64>() / active_idx.len().max(1) as f64;
let score = effective_alpha * sharpe - (1.0 - effective_alpha) * rp_div_normalized;
```

### 1-5. prediction_accuracy.rs: per-token confidence 計算

```rust
/// トークンごとの prediction confidence を計算
pub async fn calculate_per_token_confidence(
    tokens: &[TokenOutAccount],
    cfg: &impl ConfigAccess,
) -> BTreeMap<TokenOutAccount, f64> {
    let log = DEFAULT.new(o!("function" => "calculate_per_token_confidence"));
    let window = cfg.prediction_accuracy_window();
    let min_samples = cfg.prediction_accuracy_min_samples();
    let mape_excellent = cfg.prediction_mape_excellent();
    let mape_poor = cfg.prediction_mape_poor();

    // 1回の DB クエリで全トークンのレコードを取得（N クエリではなく）
    let all_records = match PredictionRecord::get_recent_evaluated_for_tokens(
        window * tokens.len() as i64, tokens
    ).await {
        Ok(r) => r,
        Err(e) => {
            warn!(log, "failed to get prediction records"; "error" => %e);
            return BTreeMap::new();
        }
    };

    // Rust 側でトークンごとにグルーピング
    let mut by_token: BTreeMap<String, Vec<_>> = all_records.into_iter()
        .fold(BTreeMap::new(), |mut map, r| {
            map.entry(r.token.clone()).or_insert_with(Vec::new).push(r);
            map
        });

    // 各トークン内を target_time DESC でソート（方向判定に必要）
    // DB は evaluated_at DESC で返すが、予測対象時点順でなければ windows(2) が正しく動かない
    for entries in by_token.values_mut() {
        entries.sort_by(|a, b| b.target_time.cmp(&a.target_time));
        entries.truncate(window as usize);  // 各トークン window 件に切り詰め
    }

    let mut result = BTreeMap::new();

    for token in tokens {
        let token_str = token.to_string();
        let records = by_token.get(&token_str);
        let mape_values: Vec<f64> = records
            .map(|rs| rs.iter().filter_map(|r| r.mape).collect())
            .unwrap_or_default();

        if mape_values.len() < min_samples {
            // データ不足 → エントリなし（後でフォールバック処理）
            // ※ 0.0 ではなくエントリなしとすることでコールドスタートを区別
            continue;
        }

        let avg_mape = mape_values.iter().sum::<f64>() / mape_values.len() as f64;

        // 方向正解率も per-token で計算
        let direction_data = records.map(|rs| {
            calculate_direction_accuracy_for_records(rs)
        });
        // 方向正解率: サンプル不足時は MAPE のみで confidence 計算（統計的有意性確保）
        let hit_rate = direction_data
            .and_then(|(correct, total)| {
                if total >= min_samples { Some(correct as f64 / total as f64) } else { None }
            });

        let confidence = calculate_composite_confidence(avg_mape, hit_rate, mape_excellent, mape_poor);

        debug!(log, "token prediction confidence";
            "token" => %token_str,
            "avg_mape" => format!("{:.2}%", avg_mape),
            "hit_rate" => hit_rate.map(|h| format!("{:.1}%", h * 100.0)),
            "confidence" => format!("{:.3}", confidence)
        );

        result.insert(token.clone(), confidence);
    }

    result
}
```

**NaN ガード**: `mape_to_confidence()` に `poor == excellent` のガードを追加:
```rust
fn mape_to_confidence(mape: f64, excellent: f64, poor: f64) -> f64 {
    let range = poor - excellent;
    if range.abs() < 1e-9 {  // f64::EPSILON は小さすぎるため 1e-9 を使用
        return if mape <= excellent { 1.0 } else { 0.0 };
    }
    ((poor - mape) / range).clamp(0.0, 1.0)
}
```

**方向正解率ヘルパー**: `calculate_direction_accuracy_for_records()` は
既存の `evaluate_pending_predictions()` 内のロジック（L253-286）の単純抽出ではなく、
**N+1 クエリを排除した再実装**。既存コードは各レコードに対して
`PredictionRecord::get_previous_evaluated()` を個別 await するが、
新ヘルパーはソート済みスライスの隣接レコード（`windows(2)`）を比較して DB アクセスなしで計算:
```rust
fn calculate_direction_accuracy_for_records(
    records: &[DbPredictionRecord],  // target_time DESC ソート済み
) -> (usize, usize) {
    let mut correct = 0usize;
    let mut total = 0usize;
    for pair in records.windows(2) {
        let (Some(actual), Some(prev_actual)) =
            (&pair[0].actual_price, &pair[1].actual_price) else { continue };
        if is_direction_correct(prev_actual, &pair[0].predicted_price, actual) {
            correct += 1;
        }
        total += 1;
    }
    (correct, total)
}
```

### 1-6. strategy.rs: confidence の構築と受け渡し

`execute_portfolio_strategy()` 内:

```rust
// 変更前 (L410-412): バックグラウンドで全体 confidence 計算
let eval_handle = tokio::spawn(async move {
    super::prediction_accuracy::evaluate_pending_predictions(&eval_cfg).await
});

// 変更後:
// evaluate_pending_predictions() は start() の冒頭で事前実行済み（下記 1-8 参照）
// execute_portfolio_strategy() 内で per-token confidence 計算 + フィルタ（上記 1-0 参照）

// token_data フィルタ後、PortfolioData に confidence を設定
// prediction_confidences にはフィルタ後トークンの confidence のみ含まれる
let filtered_confidences: BTreeMap<TokenOutAccount, f64> = prediction_confidences
    .into_iter()
    .filter(|(k, _)| token_data.iter().any(|t| &t.symbol == k))
    .collect();

let portfolio_data = PortfolioData {
    tokens: token_data,
    predictions,
    historical_prices,
    prediction_confidences: filtered_confidences,  // 変更: 単一 → per-token
};
```

### 1-7. evaluate_pending_predictions() の戻り値変更

```rust
// 変更前: 評価 + confidence 計算
pub async fn evaluate_pending_predictions(cfg: &impl ConfigAccess) -> Result<Option<(f64, f64)>>

// 変更後: 評価のみ（confidence 計算は別関数に移行）
pub async fn evaluate_pending_predictions(cfg: &impl ConfigAccess) -> Result<u32>
// 戻り値: 評価したレコード数
```

rolling MAPE/confidence の計算ロジック（L234-312）を削除し、
`calculate_per_token_confidence()` に移行。
※ `cleanup_old_records()` 呼び出し（L229-232）は残す。

### 1-8. evaluate_pending_predictions() を start() の冒頭に分離

`evaluate_pending_predictions()` は未評価レコードの DB 更新（ハウスキーピング）であり、
トレード戦略の実行とは無関係。`start()` の冒頭で事前実行する。

```rust
// strategy.rs start() の冒頭（L69 の後、manage_evaluation_period の前）
info!(log, "evaluating pending predictions");
match super::prediction_accuracy::evaluate_pending_predictions(cfg).await {
    Ok(count) => {
        if count > 0 {
            info!(log, "evaluated pending predictions"; "count" => count);
        }
    }
    Err(e) => {
        warn!(log, "prediction evaluation failed, continuing"; "error" => %e);
    }
}
```

### 期待効果

| トークン | MAPE | confidence | 判定 | alpha | 効果 |
|---------|------|------------|------|-------|------|
| blackdragon | 2.3% | 0.97 | **投資対象** | ~0.87 | Sharpe 信頼 → 安定配分 |
| ftv2 | 1.8% | 0.98 | **投資対象** | ~0.88 | Sharpe 信頼 → 安定配分 |
| nearkat | 25.6% | 0.00 | **除外** | — | 予測エッジなし → 投資しない |
| fm-1 | 22,733% | 0.00 | **除外** | — | 予測エッジなし → 投資しない |

### テスト
- per-token alpha でトークンごとに異なるブレンド比率になることの確認
- confidence < 0.3 のトークンが除外されることの確認
- 全トークン除外時にフォールバック（confidence 上位2件復帰）が動作することの確認
- Phase 2 枝刈り変更の前後で Phase 3 候補セットが妥当であることのテスト
- `mape_to_confidence(5.0, 3.0, 3.0)` — poor==excellent のエッジケーステスト
- 既存の最適化テストの assertion 更新
- `cargo test -p common -p trade`

---

## 改善案 2: ボラティリティ計算のスケールバイアス修正

### 問題
`crates/persistence/src/token_rate.rs:399` の `var_pop(rate)` はスケール依存。
`rate = tokens_yocto / NEAR` はトークンの decimals に依存してスケールが異なるため、
rate の絶対値が大きいトークンが常にボラティリティ上位に来る（スケールバイアス）。

### 対象ファイル
- `crates/persistence/src/token_rate.rs:396-409`

### 実装: 変動係数（CV）に変更

```sql
-- 変更前
SELECT base_token, var_pop(rate) as variance
FROM token_rates
WHERE quote_token = $1 AND timestamp >= $2 AND timestamp <= $3
GROUP BY base_token
HAVING MIN(rate) > 0
ORDER BY variance DESC

-- 変更後: 変動係数（CV = stddev / mean）でスケール正規化
SELECT base_token,
       stddev_pop(rate) / NULLIF(avg(rate), 0) as variance
FROM token_rates
WHERE quote_token = $1 AND timestamp >= $2 AND timestamp <= $3
GROUP BY base_token
HAVING MIN(rate) > 0 AND COUNT(*) >= 3
ORDER BY variance DESC
```

`VolatilityResult.variance` フィールドの意味が「絶対分散」から「変動係数」に変わるが、
呼び出し側は降順ソートのみ使用するため互換性に影響なし。

### テスト
- `cargo test -p persistence`

---

## 改善案 3: ALPHA_FLOOR — 変更なし（0.5 維持）

改善案1の confidence フィルタ（閾値 0.3）で低 confidence トークンは除外済み。
フィルタを通過するトークンは confidence ≥ 0.3 であり、「予測にエッジがある」と判断されたもの。

ALPHA_FLOOR=0.5（現状維持）なら confidence=0.3 で alpha=0.59（Sharpe 59%）となり、
予測を適度に活用する。**変更不要**。

---

## 改善案 4: prediction_record のトークン限定クエリ

### 対象ファイル
- `crates/persistence/src/prediction_record.rs`

### 実装
```rust
pub async fn get_recent_evaluated_for_tokens(
    limit: i64,
    tokens: &[TokenOutAccount],  // ドメイン型で受け取り、内部で String 変換
) -> Result<Vec<DbPredictionRecord>> {
    // 空配列ガード: Diesel の eq_any に空 Vec を渡すと PostgreSQL 構文エラー
    if tokens.is_empty() {
        return Ok(Vec::new());
    }
    let tokens: Vec<String> = tokens.iter().map(|t| t.to_string()).collect();
    let conn = connection_pool::get().await?;
    let results = conn
        .interact(move |conn| {
            prediction_records::table
                .filter(prediction_records::evaluated_at.is_not_null())
                .filter(prediction_records::token.eq_any(&tokens))
                .order_by(prediction_records::evaluated_at.desc())
                .limit(limit)
                .load::<DbPredictionRecord>(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;
    Ok(results)
}
```

### テスト
- `cargo test -p persistence`

---

## 実装順序

```
Step 0: is_new_period バグ修正                         [execution.rs]
Step 1: ボラティリティ CV 正規化                        [token_rate.rs]
Step 2: NaN ガード追加                                 [prediction_accuracy.rs]
Step 3: evaluate_pending_predictions 分離              [prediction_accuracy.rs, strategy.rs]
          - 戻り値 Result<Option<(f64,f64)>> → Result<u32>
          - rolling MAPE/confidence 計算ロジック（L234-312）を削除
            ※ cleanup_old_records() 呼び出し（L229-232）は残す
          - start() 冒頭での同期実行に移動
          - strategy.rs: tokio::spawn 削除 + L621-641 パターンマッチ削除
            + prediction_confidence を None にハードコード（中間状態）
Step 4: per-token confidence + フィルタ + alpha         [prediction_accuracy.rs, prediction_record.rs,
                                                        portfolio.rs, strategy.rs, typed.rs]
          - typed.rs: TRADE_MIN_TOKEN_CONFIDENCE 設定追加
          - prediction_record: get_recent_evaluated_for_tokens 追加
          - prediction_accuracy: calculate_per_token_confidence 追加
          - prediction_accuracy: calculate_direction_accuracy_for_records 追加（N+1排除の再実装）
          - strategy.rs: confidence フィルタ + フォールバック（最低2トークン保証）
          - portfolio.rs: PortfolioData 変更、per-token alpha 対応
          - strategy.rs: execute_portfolio_strategy() フロー変更
          - テスト更新（約27箇所）:
            PortfolioData 構築(12), unified_optimize 呼び出し(9),
            exhaustive_optimize 呼び出し(1), prediction_confidence テスト書き換え(4),
            golden output テスト再計算(1)
```

Step 0, 1, 2 は独立してコミット可能。
Step 3 は evaluate 関数の責務分離のみ。prediction_confidence=None のハードコードは
Step 4 で解消される。
Step 4 は PortfolioData の型変更が portfolio.rs と strategy.rs に波及するため1コミット。
Step 3+4 は同一 PR にすること。

### 既知の制限・将来課題

- **Sharpe ウエイト振動**: 高 alpha トークン同士の Sharpe ウエイトが日次で入れ替わると
  threshold=0.10 を超えてリバランスが発生しうる。将来的に Sharpe ウエイトの
  指数移動平均化で日次ノイズを平滑化することを検討
- **exhaustive_optimize の alpha 平均バイアス**: 高 confidence サブセットが体系的に
  有利になり、低 confidence トークンが「RP フォールバック」ではなく「排除」される可能性。
  対策案: スコア計算に固定 alpha_vol を使い、per-token alpha は blend のみに適用して
  「サブセット選択」と「ウエイトブレンド」を分離する。運用データで問題が確認されたら対応
- **normalize 後の RP ウエイト膨張**: 中程度 confidence のトークンで RP ウエイトが大きい場合、
  normalize で高 alpha トークンのウエイトが圧縮される可能性あり。低 confidence トークンは
  フィルタで除外されるため影響は限定的だが、中 confidence (0.3-0.6) のトークンでは起こりうる

## シミュレーション結果（過去10日間のデータ: 2026-03-07 〜 2026-03-17）

### Per-token confidence と選定結果

| トークン | MAPE | confidence | 判定 | 理由 |
|---------|------|-----------|------|------|
| blackdragon | 2.26% | 1.000 | **投資対象** | 高精度の予測でエッジあり |
| ftv2 | 1.77% | 1.000 | **投資対象** | 高精度の予測でエッジあり |
| nearkat | 25.61% | 0.000 | **除外** | 予測が当たらない → エッジなし |
| fm-1 | 22,733% | 0.000 | **除外** | 予測が壊滅的 → 投資不可 |

### 推定ウエイト（低 confidence 除外後）

| トークン | 現状ウエイト | 改善後ウエイト |
|---------|------------|--------------|
| blackdragon | 35.4% | **~54%** |
| ftv2 | 37.8% | **~46%** |
| nearkat | 26.7% | **0%（除外）** |

### パフォーマンス比較

| 戦略 | 最終資産 | リターン |
|------|---------|---------|
| **低confidence除外（改善案）** | **21.88 NEAR** | **+9.9%** |
| per-token alpha + RP 4.5% | 21.60 NEAR | +8.5% |
| 均等配分 buy-and-hold | 20.12 NEAR | +1.1% |
| **実際の結果** | **14.06 NEAR** | **-29.4%** |

**改善効果: +39.3% のリターン改善**（-29.4% → +9.9%）

※ シミュレーションは buy-and-hold 近似（手数料未控除）。実運用では日次でウエイト再計算されるため、
Sharpe ウエイト振動によるリバランスコスト（推定 0.3-0.9%）が発生する可能性あり。
実質的なリターンは +9.0-9.6% 程度と推定。

### 効果の内訳

1. **チャーンコスト排除**: ウエイト安定 → リバランス不要 → スリッページ・手数料ゼロ
2. **nearkat 完全除外**: -18.3% の下落の影響がゼロ（RP 配分もなし）
3. **ftv2 の高ウエイト維持**: +21.4% の上昇をより多く取得

---

## 検証方法

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. DB でシミュレーション:
   ```sql
   SELECT token,
          ROUND(AVG(mape)::numeric, 2) as avg_mape,
          CASE
            WHEN AVG(mape) <= 3 THEN 'HIGH (Sharpe trust)'
            WHEN AVG(mape) <= 15 THEN 'MEDIUM'
            ELSE 'LOW (excluded)'
          END as confidence_level
   FROM prediction_records
   WHERE evaluated_at IS NOT NULL
   AND created_at >= '2026-03-07'
   GROUP BY token ORDER BY avg_mape;
   ```
5. テスト環境で1評価期間（10日）実行して効果を測定
