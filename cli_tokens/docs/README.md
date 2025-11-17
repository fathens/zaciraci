# CLI Tokens ドキュメント

このディレクトリには、CLI Tokensツールの詳細ドキュメントが含まれています。

## 📖 ドキュメント一覧

### コマンドリファレンス
- **[predict.md](./predict.md)** - 全コマンドの詳細仕様（1125行）
  - 注：将来的にコマンド別ファイルに分割予定
- **[simulate.md](./simulate.md)** - シミュレーションコマンドの詳細

### 設計ドキュメント
- **[cache_design.md](./cache_design.md)** - データキャッシュ設計仕様
- **[rules.md](./rules.md)** - テストデータとワークファイルの管理ルール

## 🚀 クイックスタート

### 主要コマンド

#### 1. top - ボラティリティトップトークン取得
```bash
cli_tokens top --start 2025-01-01 --end 2025-01-31 --limit 10
```

#### 2. history - 価格履歴取得
```bash
cli_tokens history <token_id> --quote-token wrap.near
```

#### 3. predict - 価格予測
```bash
cli_tokens predict <token_id> --model chronos_default
```

#### 4. verify - 予測精度検証
```bash
cli_tokens verify <prediction_file>
```

#### 5. simulate - バックテストシミュレーション
```bash
cli_tokens simulate --start 2025-01-01 --end 2025-01-31
```

詳細は [simulate.md](./simulate.md) を参照。

#### 6. report - レポート生成
```bash
cli_tokens report <results_file> --format html
```

#### 7. chart - チャート生成
```bash
cli_tokens chart <data_file> --output chart.png
```

## 📁 ワークディレクトリ構造

```
cli_tokens/.work/
├── tokens/                 # トークンメタデータ
│   └── wrap.near/         # クォートトークン別
│       └── <token>.json
├── price_history/         # 価格履歴データ
│   └── wrap.near/
│       └── <token>/
│           └── history-<range>.json
├── predictions/           # 予測データ
│   └── wrap.near/
│       └── <token>/
│           └── predict-<range>-<model>.json
├── simulation_results/    # シミュレーション結果
│   └── <algorithm>_<range>/
│       ├── results.json
│       ├── report.html
│       └── chart.png
└── cache/                # キャッシュデータ
```

詳細は [cache_design.md](./cache_design.md) を参照。

## 🔧 環境変数

### 必須
- `CLI_TOKENS_BASE_DIR`: ワークディレクトリのベースパス（デフォルト: `cli_tokens/.work`）
- `BACKEND_URL`: Backend APIのURL（例: `http://localhost:3000`）
- `CHRONOS_URL`: Chronos APIのURL（例: `http://localhost:8000`）

### オプション
- `RUST_LOG`: ログレベル（例: `debug`, `info`, `warn`, `error`）

## 🧪 テストデータ管理

テストデータの管理ルールについては [rules.md](./rules.md) を参照してください。

## 📊 アーキテクチャ

### データフロー
1. **top**: Backend API → ボラティリティトークン取得
2. **history**: Backend API → 価格履歴取得 → キャッシュ保存
3. **predict**: Chronos API → 価格予測 → キャッシュ保存
4. **simulate**: history + predict → バックテスト実行 → レポート生成

### キャッシュ戦略
- 価格履歴: 時間範囲ごとにキャッシュ
- 予測データ: モデル・時間範囲ごとにキャッシュ
- 自動キャッシュ無効化: データの鮮度に基づく

詳細は [cache_design.md](./cache_design.md) を参照。

## 🔗 関連ドキュメント

- [CONTRIBUTING.md](../../CONTRIBUTING.md) - 開発ガイドライン
- [diagram/trade/](../../diagram/trade/) - トレーディングシステム設計

## ⚠️ 注意事項

- **本番環境**: 小額から開始し、十分なテストを実施してください
- **データ管理**: `.work`ディレクトリはGitで管理されません（.gitignore設定済み）
- **APIレート制限**: Backend/Chronos APIのレート制限に注意してください
