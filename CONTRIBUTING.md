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

## プロジェクトアーキテクチャ

Zaciraciは、NEAR ブロックチェーン上でのDeFi裁定取引を行うRust製のフルスタックWebアプリケーションです。

### ワークスペース構成
- **backend**: Axum ベースのREST APIサーバー（NEAR ブロックチェーン連携、裁定取引計算、データベース操作）
- **frontend**: Dioxus ベースのWebAssemblyフロントエンド（取引インターフェース、プール可視化、AI予測）
- **common**: バックエンドとフロントエンドで共有される型、設定、ユーティリティ
- **../zcrc-chronos**: Chronos時系列予測APIサーバー（フロントエンドから予測リクエストを受信）

### 主要コンポーネント
- **裁定取引エンジン** (`backend/src/arbitrage.rs`, `backend/src/trade/`): 取引アルゴリズムとARIMA統計分析
- **REF Finance連携** (`backend/src/ref_finance/`): NEAR DeFiプロトコル連携（プール分析、スワップ、残高管理）
- **データベース層** (`backend/src/persistence/`): Diesel ORMを使用したPostgreSQL連携
- **AI統合** (`backend/src/ollama/`, `frontend/src/ollama.rs`): ローカルLLMによる取引予測と分析
- **Webインターフェース** (`backend/src/web/`, `frontend/src/`): REST APIとリアクティブWeb UI

## 開発環境セットアップ

### 前提条件
- Rust（バージョンは rust-toolchain.toml を参照）
- Docker と Docker Compose
- diesel_cli (`cargo install diesel_cli --no-default-features --features postgres`)

### ローカル開発環境
```bash
# ローカル環境を起動（PostgreSQL + バックエンド）
cd run_local
./run.sh

# バックエンドは http://localhost:8080 で起動
# フロントエンド開発は別途 trunk serve を使用
```

### 環境変数設定
主要な環境変数（`run_local/docker-compose.yml`参照）:
- `PG_DSN`: PostgreSQL接続文字列
- `USE_MAINNET`: NEAR mainnet/testnet切り替え
- `ROOT_MNEMONIC`, `ROOT_ACCOUNT_ID`: NEARウォレット設定
- `OLLAMA_BASE_URL`, `OLLAMA_MODEL`: AIモデル設定
- `RUST_LOG`: ログレベル設定

### データベース環境
PostgreSQLはDockerで管理されます:
```yaml
# run_local/docker-compose.yml の postgres サービス
postgres:
  image: postgres:15-bookworm
  ports:
    - "5432:5432"
  environment:
    - POSTGRES_USER=postgres
    - POSTGRES_PASSWORD=postgres
```

### テスト環境
```bash
# テスト用PostgreSQL環境を起動
cd run_test
./run.sh

# テスト用データベースでテスト実行
PG_DSN=postgres://postgres_test:postgres_test@localhost:5433/postgres_test cargo test -- --nocapture
```

### フロントエンド開発
フロントエンドはDioxusを使用したWebAssemblyアプリケーション:
```bash
cd frontend
trunk serve  # 通常は http://localhost:8080 で起動
```

### データベースマイグレーション
- データベーススキーマ変更にはDieselを使用
- `diesel migration run` でマイグレーションを実行
- `diesel migration generate <name>` で新しいマイグレーションを作成
- スキーマは `src/persistence/schema.rs` で定義