# データキャッシュ設計仕様

cli_tokensの`history`・`predict`・`simulate`コマンドで使用する共通データ管理システムの**実装者向け設計仕様書**です。

## このドキュメントの目的

- **対象読者**: cli_tokensの開発者・実装者
- **目的**: キャッシュシステムの内部設計と実装方針を定義
- **関連文書**: 
  - `predict.md` - エンドユーザー向けのコマンド使用方法
  - `simulate.md` - simulateコマンドの詳細仕様

## 概要

以下のAPIデータを統一されたディレクトリ構造で管理します：
- **価格履歴データ**: バックエンドAPIから取得される時系列価格データ
- **予測データ**: Chronos APIからの予測結果

これらのデータをファイルシステムに保存・再利用することで、API呼び出しを削減し、コマンド間でデータを共有します。

## ディレクトリ構造

```
{CLI_TOKENS_BASE_DIR}/
├── price_history/
│   └── {quote_token}/
│       └── {base_token}/
│           ├── history-{start}-{end}.json
│           └── history-{start}-{end}.json
└── predictions/
    └── {model_name}[_{params_hash}]/
        └── {quote_token}/
            └── {base_token}/
                └── history-{hist_start}-{hist_end}/
                    ├── predict-{pred_start}-{pred_end}.json
                    └── predict-{pred_start}-{pred_end}.json
```

## ファイル名形式

### 時刻フォーマット
```
YYYYMMDD_HHMM
```

例：`20250801_0000` (2025年8月1日 00:00 UTC)

### 期間の区切り
ハイフン（`-`）を「from-to」の意味で使用

例：`20250801_0000-20250807_2359` (2025年8月1日00:00から8月7日23:59まで)

### 価格履歴データ
```
history-{開始時刻}-{終了時刻}.json
```

例：
- `history-20250801_0000-20250807_2359.json`
- `history-20250815_1200-20250820_1200.json`

### 予測データ
```
predict-{予測開始時刻}-{予測終了時刻}.json
```

例：
- `predict-20250808_0000-20250809_0000.json`
- `predict-20250808_1200-20250809_1200.json`

## 具体的な例

### 価格履歴データ
```
price_history/wrap.near/akaia.tkn.near/
├── history-20250801_0000-20250807_2359.json
├── history-20250815_1200-20250820_1200.json
└── history-20250901_0000-20250930_2359.json
```

### 予測データ
```
predictions/chronos_default/wrap.near/akaia.tkn.near/
├── history-20250801_0000-20250807_2359/
│   ├── predict-20250808_0000-20250809_0000.json
│   ├── predict-20250808_1200-20250809_1200.json
│   └── predict-20250810_0000-20250811_0000.json
└── history-20250815_0000-20250821_2359/
    └── predict-20250822_0000-20250823_0000.json
```

### カスタムパラメータ付きモデル
```
predictions/chronos_default_a1b2c3d4/wrap.near/akaia.tkn.near/
└── history-20250801_0000-20250807_2359/
    └── predict-20250808_0000-20250809_0000.json
```

## キャッシュキーの構成要素

### 価格履歴データ
- `quote_token`: 見積りトークン（例：wrap.near）
- `base_token`: 対象トークン（例：akaia.tkn.near）
- `start_time`: データ開始時刻
- `end_time`: データ終了時刻

### 予測データ
- `model_name`: 予測モデル名（例：chronos_default）
- `params_hash`: カスタムパラメータのSHA256ハッシュの先頭8文字（オプション）
- `quote_token`: 見積りトークン
- `base_token`: 対象トークン
- `history_start`: 履歴データ開始時刻
- `history_end`: 履歴データ終了時刻
- `forecast_start`: 予測開始時刻
- `forecast_end`: 予測終了時刻

## パラメータハッシュ

`model_params`にカスタムパラメータが設定されている場合、そのJSON文字列のSHA256ハッシュを計算し、先頭8文字をモデルディレクトリ名に追加します。

### 例
```json
{
    "temperature": 0.8,
    "top_p": 0.9,
    "confidence_threshold": 0.85
}
```
↓
`SHA256(JSON文字列)` = `a1b2c3d4e5f6...`
↓
モデルディレクトリ名：`chronos_default_a1b2c3d4`

## データ共有の利点

1. **API呼び出し削減**: 同一条件での重複リクエストを回避
2. **パフォーマンス向上**: ローカルファイルアクセスによる高速化
3. **コマンド間連携**: 異なるコマンドで同じデータを再利用
4. **オフライン作業**: 一度取得したデータでの作業が可能
5. **コスト削減**: API使用量の削減

## データ管理

### TTL（Time To Live）
データの有効期限を設定可能（デフォルト：24時間）

### 強制更新
`--force`フラグで強制的にAPIから再取得

### 部分データ利用
より大きな期間のデータから部分データを抽出する機能（将来の拡張）

## 実装ガイドライン

### キャッシュ検索アルゴリズム

```rust
// 価格履歴データの検索
fn find_cached_price_history(
    quote_token: &str,
    base_token: &str, 
    start: DateTime<Utc>,
    end: DateTime<Utc>
) -> Option<PathBuf> {
    // 1. 完全一致を検索
    let exact_match = format!("history-{}-{}.json", 
        format_time(start), format_time(end));
    
    // 2. 期間を含むより大きなデータを検索
    // 3. 複数の小さな期間を結合できるか確認
}

// 予測データの検索
fn find_cached_prediction(
    model: &str,
    params_hash: Option<&str>,
    history_period: &Period,
    prediction_period: &Period
) -> Option<PathBuf> {
    // モデル+パラメータディレクトリを特定
    // 履歴期間ディレクトリを検索
    // 予測期間ファイルを検索
}
```

### 実装における注意点

1. **一意性の保証**: 全てのパラメータが一致する場合のみ既存データを利用
2. **ファイルロック**: 同時アクセス時の整合性保証
   ```rust
   use fs2::FileExt;
   let file = File::open(path)?;
   file.lock_shared()?; // 読み込み時
   file.lock_exclusive()?; // 書き込み時
   ```
3. **エラー処理**: データ読み込み失敗時のフォールバック
4. **ディスク容量**: 古いデータの自動削除機能
5. **部分データ利用**: より大きな期間のキャッシュから必要な部分を抽出

## 各コマンドでの利用方法

### historyコマンド
価格履歴データを`price_history/`ディレクトリに保存・読み込み

### predictコマンド  
予測データを`predictions/`ディレクトリに保存・読み込み

### simulateコマンド
両方のディレクトリからデータを読み込み、必要に応じて新規取得・保存

## 移行について

既存の`history`コマンド出力ディレクトリからの移行が必要：
```
# 従来
{CLI_TOKENS_BASE_DIR}/history/{quote_token}/{base_token}.json

# 新形式
{CLI_TOKENS_BASE_DIR}/price_history/{quote_token}/{base_token}/history-{start}-{end}.json
```