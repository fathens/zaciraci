# Zaciraci 開発ガイドライン

## 開発ルール

### コードスタイル
- `cargo fmt --all -- --check` でRustコードフォーマットをチェック
- `cargo clippy --all-targets --all-features -- -D warnings` でlintをチェック（警告はエラーとして扱う）
- `cargo test` ですべてのテストが通ることを確認

### CI/CDチェック項目
開発時は以下のコマンドでCIと同じチェックを実行可能:

1. **フォーマットチェック**
   ```bash
   cargo fmt --all -- --check
   ```

2. **Clippy（静的解析）**
   ```bash
   cargo clippy --all-targets --all-features -- -D warnings
   ```

3. **テスト実行**
   ```bash
   cargo test
   ```

### 必要な依存関係
- システム依存: `libfontconfig1-dev`, `pkg-config`
- Rustコンポーネント: `rustfmt`, `clippy`, `llvm-tools-preview`
- 追加ツール: `diesel_cli`

### テスト
- 新機能には単体テストを作成
- `cargo test` でテストを実行
- テストカバレッジを維持

### コミットメッセージ
- 明確で説明的なコミットメッセージを使用
- 可能であれば conventional commit 形式に従う

### ブランチ戦略
- Git Flow を採用
- `develop` ブランチが開発の中心
- `main` ブランチは本番リリース用
- 機能開発は `feature/*` ブランチで行う
- リリースは `release/*` ブランチで準備
- 緊急修正は `hotfix/*` ブランチで対応

### プルリクエスト
- develop ブランチから機能ブランチを作成
- レビュー依頼前にCIが通ることを確認
- 変更内容とテスト方法の説明を含める

## 開発環境セットアップ

### 前提条件
- Rust（バージョンは rust-toolchain.toml を参照）
- ローカル開発用の Docker と Docker Compose
- データベース操作用の PostgreSQL

### ローカル開発
```bash
# ローカル環境を起動
cd run_local
./run.sh

# テストを実行
cd run_test
./run.sh
```

### データベースマイグレーション
- データベーススキーマ変更にはDieselを使用
- `diesel migration run` でマイグレーションを実行
- `diesel migration generate <name>` で新しいマイグレーションを作成