# ゼロショット予測システムのフロー図

## 全体システム構成

```
[CLI Tool (Rust)] ←→ [Chronos API (Python/FastAPI)] ←→ [AutoGluon]
        ↓                        ↓                           ↓
   - トークン分析        - データ前処理              - 機械学習モデル
   - 予測実行           - 正規化/逆正規化           - 予測実行
   - 結果出力           - 非同期処理                - モデル選択
```

## 詳細データフロー

### 1. CLI Tool → Chronos API

```
[CLI: predict subcommand]
    ↓ 1. トークンデータ取得 (データベースから)
    ↓ 2. API呼び出し (chronos_api/predict.rs)
    ↓
[HTTP POST /predict_zero_shot]
{
  "token": "akaia.tkn.near", 
  "values": [価格データ配列],
  "timestamps": [タイムスタンプ配列],
  "forecast_horizon": 24
}
```

### 2. Chronos API内部処理

```
[routes.py: predict_zero_shot()]
    ↓ 1. データ検証・前処理
    ↓ 2. 正規化処理 (90:10分割)
    ↓ 3. AutoGluon予測実行
    ↓ 4. 結果の逆正規化
    ↓ 5. レスポンス生成

[predictor.py: predict_zero_shot()]
    ↓ 1. データサイズ確認 (23ポイント)
    ↓ 2. 予測期間調整 (24h → 3h)
    ↓ 3. AutoGluon設定
    ↓ 4. モデル学習・予測
    ↓ 5. 結果返却

[AutoGluon]
    ↓ 1. モデル選択 (除外: Naive, RecursiveTabular)
    ↓ 2. 学習実行 (medium_quality, 30秒制限)
    ↓ 3. 予測実行 (719個の予測値生成)
    ↓ 4. 結果返却
```

### 3. Chronos API → CLI Tool

```
[HTTP Response]
{
  "model_name": "chronos_default",
  "forecast_values": [同じ値が719個...],  ← ここが問題！
  "forecast_timestamp": [タイムスタンプ配列],
  "metrics": {...}
}
    ↓
[CLI: predict subcommand]
    ↓ 1. API応答受信
    ↓ 2. 結果分析・統計計算
    ↓ 3. 標準出力/ファイル出力
    ↓ 4. CSVエクスポート (オプション)
```

## 現在の問題分析

### 問題1: AutoGluonが同じ値を返す

```
AutoGluon学習データ:
- 入力: 23ポイント (6時間間隔)
- 90:10分割 → 学習: 20.7ポイント, テスト: 2.3ポイント
- 予測期間: 3時間 (データ不足のため24時間から縮小)

結果:
- 全予測値が同一 (例: 165086991646.08908 × 719回)
- 最小値 = 最大値 = 平均値
```

### 問題2: モデル選択の問題

```
除外されたモデル:
✗ Naive (単純すぎる)
✗ RecursiveTabular (同じ値を返す)

使用可能モデル:
✓ ETS
✓ SeasonalNaive  
✓ Theta
✓ SimpleFeedForward
✓ Chronos

実際に選択されるモデル: ???
→ ログで確認が必要
```

### 問題3: データ品質

```
元データの特徴:
- 24時間で23ポイント (6時間間隔)
- AutoGluon最小要件: horizon + 5 = 8ポイント
- 実際: 23ポイント > 8ポイント (十分)

しかし:
- 短期間での予測 (3時間)
- 少ないバリエーション
- 正規化による情報損失の可能性
```

## 修正が必要な箇所

### 1. AutoGluon設定の改善
```python
# predictor.py
- time_limit: 30秒 → 60秒以上
- presets: "medium_quality" → "good_quality"
- 明示的なモデル指定
```

### 2. データ処理の改善
```python
# routes.py
- 正規化アルゴリズムの見直し
- データ拡張の検討
- 異なるバリデーション手法
```

### 3. ログ出力の強化
```python
# 使用されたモデル名の出力
# 学習データの統計情報
# 予測過程の詳細ログ
```

## 期待される改善後のフロー

```
[AutoGluon] 
    ↓ 適切なモデル選択 (SimpleFeedForward等)
    ↓ 十分な学習時間
    ↓ バリエーションのある予測値生成
    ↓
[719個の異なる予測値] → [統計分析] → [CLI出力]
```

## 次のアクション

1. **AutoGluonログの確認**: 実際に選択されているモデル
2. **設定調整**: time_limit, presets, hyperparameters
3. **データ処理改善**: 正規化アルゴリズム
4. **テスト実行**: 修正後の動作確認