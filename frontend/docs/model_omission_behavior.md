# モデル指定省略時の動作検証

## 概要

Chronos API でモデル名を省略した場合の動作について検証し、設定オプションを追加しました。

## 実装内容

### 1. 設定オプション

新しい環境変数 `PREDICTION_OMIT_MODEL_NAME` を追加：

```bash
# モデル名を省略してサーバーのデフォルトモデルを使用
PREDICTION_OMIT_MODEL_NAME=true

# モデル名を明示的に指定（デフォルト）
PREDICTION_OMIT_MODEL_NAME=false
```

### 2. JSON リクエストの違い

#### モデル名を指定した場合
```json
{
  "timestamp": ["2023-01-01T00:00:00Z", "2023-01-01T01:00:00Z"],
  "values": [10.5, 11.2],
  "forecast_until": "2023-01-04T02:00:00Z",
  "model_name": "chronos-bolt-base"
}
```

#### モデル名を省略した場合
```json
{
  "timestamp": ["2023-01-01T00:00:00Z", "2023-01-01T01:00:00Z"],
  "values": [10.5, 11.2],
  "forecast_until": "2023-01-04T02:00:00Z"
}
```

### 3. サーバー側での実際の動作（調査結果）

**🔍 実際の調査結果に基づく動作：**

1. **FastAPIレベル**: `model_name: Optional[str] = "chronos_default"`
   - モデル名が省略された場合、自動的に `"chronos_default"` が設定される

2. **実際の予測エンジン**: `AutoGluon TimeSeries DeepAR`
   - 表示名は `"chronos_default"` だが、実際は AutoGluon の DeepAR モデルを使用
   - プリセット: `medium_quality`
   - ハイパーパラメータは AutoGluon が自動最適化

3. **設定ファイルとの関係**:
   - `model_config.yaml` では `model_type: "prophet"` と設定されているが
   - 実装では `DeepAR` がハードコードされており、設定ファイルの値は現在使用されていない

## 利点とデメリット

### 利点

1. **高性能モデル**: AutoGluon TimeSeries の DeepAR による高精度予測
2. **自動最適化**: ハイパーパラメータが自動的に最適化される
3. **設定の簡素化**: 複雑なモデル設定を考慮する必要がない
4. **安定性**: 実績のある AutoGluon エコシステムの恩恵

### デメリット

1. **透明性の欠如**: 表示名と実際のモデルが異なる
2. **設定の不整合**: YAML設定ファイルの値が実際に使用されていない
3. **カスタマイズ制限**: DeepAR 以外のモデルを使用する柔軟性がない
4. **デバッグの複雑さ**: 実際の処理がブラックボックス化されている

## 推奨される使用ケース

### モデル省略が適している場合

- **プロトタイピング**: 迅速な開発・テスト
- **一般的な用途**: 特定のモデル要件がない場合
- **自動化システム**: モデル選択をサーバーに委ねたい場合

### モデル指定が適している場合

- **本番環境**: 一貫した予測結果が必要
- **パフォーマンス要件**: 特定の速度・精度要件がある
- **監査・コンプライアンス**: 使用モデルの追跡が必要

## 実装の詳細

### 設定構造体への追加

```rust
pub struct PredictionConfig {
    // ... 既存フィールド
    /// モデル指定を省略するかどうか（デフォルト: false）
    pub omit_model_name: bool,
}
```

### 条件分岐ロジック

```rust
let prediction_request = if config.omit_model_name {
    // モデル名を省略（サーバーのデフォルトモデルを使用）
    ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
} else {
    // モデル名を明示的に指定
    ZeroShotPredictionRequest::new(timestamps, values, forecast_until)
        .with_model_name(model_name)
};
```

## テスト結果

- ✅ モデル名省略時にJSONから`model_name`フィールドが除外される
- ✅ モデル名指定時に正しく`model_name`フィールドが含まれる
- ✅ シリアライズ・デシリアライズが正常に動作する

## 今後の検証事項

1. **実際のChronos APIサーバーでのテスト**: モデル省略時の実際の動作確認
2. **レスポンスの`model_name`フィールド**: サーバーが使用したモデル名の確認
3. **パフォーマンス比較**: 明示指定 vs サーバー選択の精度・速度比較
4. **エラーハンドリング**: モデル省略が許可されない場合の適切な処理

## 推奨設定

本番環境では一貫性を重視してモデルを明示的に指定することを推奨：

```bash
PREDICTION_OMIT_MODEL_NAME=false
PREDICTION_DEFAULT_MODEL=chronos-bolt-base
```

開発・テスト環境では柔軟性を重視してモデル省略も可能：

```bash
PREDICTION_OMIT_MODEL_NAME=true
```