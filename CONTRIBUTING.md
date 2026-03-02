# Zaciraci 開発ガイドライン

## 開発ルール

### コードスタイル
- `cargo fmt --all -- --check` でRustコードフォーマットをチェック
- `cargo clippy --all-targets --all-features -- -D warnings` でlintをチェック（警告はエラーとして扱う）
- `#[allow(clippy::...)]` による clippy 警告の抑制は禁止。警告が出た場合はコードを修正して根本対応すること
- `cargo test` ですべてのテストが通ることを確認

#### ドメイン型の使用

##### 原則

`String`, `BigDecimal`, `u128`, `f64` などのプリミティブ型を直接使う前に、`common::types`（`crates/common/src/types.rs` の re-export 一覧を参照）および `dex` クレートに意味のある専用型がないか確認すること。専用型が存在する場合は必ずそちらを使う。

##### 探す場所

- `crates/common/src/types.rs` — re-export 一覧（`TokenAccount`, `NearValue`, `ExchangeRate`, `TokenAmount` 等）
- `crates/common/src/types/` 配下の各ファイル — 型の定義とドキュメント
- `crates/dex/src/` — DEX ドメイン型（`PoolInfo`, `TokenPair`, `TokenPath` 等）

##### 判断基準

- 値に「単位」や「意味」があるなら専用型を使う（例: NEAR 金額、トークン ID、交換レート）
- 関数シグネチャでは特に重要 — 引数や戻り値にプリミティブ型を使うと呼び出し元全体に波及する
- コレクションのキーや要素にも適用（例: `BTreeMap<String, ...>` のキーがトークン ID なら `BTreeMap<TokenAccount, ...>` にする）

##### persistence 層の扱い

ドメイン型は Diesel の SQL カラム型トレイト（`FromSql`/`ToSql`）を実装していないため、Diesel モデル構造体の直接マッピングカラム（VARCHAR, NUMERIC 等）にはプリミティブ型を使う。ドメイン型との変換は persistence を呼び出す側で行う。

ただし、JSONB カラムではドメイン型は全て Serde（`Serialize`/`Deserialize`）を実装しているため、`serde_json::to_value()` / `from_value()` 経由で直接使用できる。

#### モジュール構成
**モダンなRustコードスタイル**: `mod.rs`ファイルの使用を避け、ディレクトリ同名のファイルを使用する

```rust
// 推奨されるモダンな構成
src/
├── lib.rs または main.rs
├── utils.rs          // utils/ ディレクトリ内の pub mod を定義
├── utils/
│   ├── config.rs     // pub mod config;
│   ├── file.rs       // pub mod file;
│   └── validation.rs // pub mod validation;
├── api.rs            // api/ ディレクトリ内の pub mod を定義
└── api/
    ├── handlers.rs   // pub mod handlers;
    ├── routes.rs     // pub mod routes;
    └── middleware.rs // pub mod middleware;

// utils.rs の内容例
pub mod config;
pub mod file;
pub mod validation;

// api.rs の内容例
pub mod handlers;
pub mod routes;
pub mod middleware;
```

```rust
// 避けるべき従来の構成
src/
├── lib.rs または main.rs
├── utils/
│   ├── mod.rs        // ← 避けるべき
│   ├── config.rs
│   ├── file.rs
│   └── validation.rs
└── api/
    ├── mod.rs        // ← 避けるべき
    ├── handlers.rs
    ├── routes.rs
    └── middleware.rs
```

この構成により、モジュールの構造がより明確になり、ファイルの役割が理解しやすくなります。

### ログ出力の方針
**重要**: `println!` マクロの使用は禁止です。適切なログマクロを使用してください。
- **例外**: テストコード（`#[cfg(test)]`モジュールや`tests.rs`ファイル）では、デバッグ出力として`println!`の使用を許可します。

#### バックエンド（backend/）
- `slog` 構造化ログライブラリを使用
- `use crate::logging::*;` でインポート
- 各関数でloggerを作成してからログ出力
- 使用例:
  ```rust
  use crate::logging::*;
  
  fn my_function() {
      let log = DEFAULT.new(o!("function" => "my_function"));
      debug!(log, "デバッグ情報"; "value" => %some_value);
      info!(log, "処理開始");
      error!(log, "エラー発生"; "error" => %error);
  }
  ```

#### テストコードでのログ
- テスト関数でも同様にloggerを作成
- デバッグ情報は `debug!` レベルを使用
- テスト結果の確認情報は適切なレベルを選択

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

### テストコードの分離

以下の **両方** を満たすファイルは、テストコードを別ファイルに分離する。

1. テストコード（`#[cfg(test)] mod tests { ... }` ブロック）がファイル全体の **1/4 超**
2. テストコードが **100 行超**

#### 分離方法

`foo.rs` を `foo.rs` + `foo/tests.rs` に分割する。`mod.rs` は使わない。

**変更前:**

```
src/
  foo.rs          # プロダクションコード + テスト
```

**変更後:**

```
src/
  foo.rs          # プロダクションコード + #[cfg(test)] mod tests;
  foo/
    tests.rs      # テストモジュールの中身（mod tests { } の内側だけ）
```

**`foo.rs` の末尾:**

```rust
#[cfg(test)]
mod tests;
```

**`foo/tests.rs`:**

```rust
use super::*;

#[test]
fn test_example() {
    // ...
}
```

#### 大規模テストファイルの分割

テストファイル（`tests.rs` や `tests/` 配下のファイル）が **2000 行**を超える場合は、テストの関心事ごとにサブモジュールへ分割すること。

**変更前:**

```
src/
  foo.rs
  foo/
    tests.rs      # 2000 行超の大規模テストファイル
```

**変更後（サブモジュール名は一例）:**

```
src/
  foo.rs
  foo/
    tests.rs      # pub use + mod 宣言のみ
    tests/
      helpers.rs
      basic.rs
      advanced.rs
```

**`tests.rs`（分割後）:**

```rust
pub use super::*;
// テスト共通の use 宣言

mod helpers;
pub use helpers::*;

mod basic;
mod advanced;
```

**各サブモジュール:**

```rust
use super::*;

#[test]
fn test_example() {
    // ...
}
```

### コミット粒度
- コミットは独立した変更ごとに分けること（1コミット = 1つの論理的変更）
- 1つのコミットに複数の独立した変更を混ぜない
- 例: 3つの独立したテスト追加 → 3つの個別コミット

後でコミットを分けるよりも、事前にコミット単位の実装順を計画して進めることを推奨する

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

Zaciraciは、NEAR ブロックチェーン上でのDeFi裁定取引を行うRust製のアプリケーションです。

### ワークスペース構成

すべてのクレートは `crates/` ディレクトリ配下に配置する。新しいクレートを追加する場合も `crates/<クレート名>/` に作成し、ルートの `Cargo.toml` の `workspace.members` に登録すること。

- **crates/backend**: バイナリオーケストレータ（main.rs のみ。各クレートを起動する）
- **crates/common**: 共有される型、設定、ユーティリティ
- **crates/dex**: DEX ドメイン型（PoolInfo、TokenPair、TokenPath 等）
- **crates/logging**: slog 構造化ロギング
- **crates/persistence**: DB接続・スキーマ・データアクセス（Diesel ORM / PostgreSQL）
- **crates/blockchain**: NEAR ブロックチェーン連携（JSON-RPC、REF Finance、ウォレット）
- **crates/trade**: ポートフォリオベース自動取引エンジン
- **crates/arbitrage**: 裁定取引エンジン

### 主要コンポーネント
- **裁定取引エンジン** (`crates/arbitrage/`): 裁定取引アルゴリズム
- **取引エンジン** (`crates/trade/`): ポートフォリオ戦略、ARIMA統計分析、予測精度評価
- **REF Finance連携** (`crates/blockchain/src/ref_finance/`): NEAR DeFiプロトコル連携（プール分析、スワップ、残高管理）
- **データベース層** (`crates/persistence/`): Diesel ORMを使用したPostgreSQL連携

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

```

### 環境変数設定
主要な環境変数（`run_local/docker-compose.yml`参照）:
- `DATABASE_URL`: PostgreSQL接続文字列
- `USE_MAINNET`: NEAR mainnet/testnet切り替え
- `ROOT_MNEMONIC`, `ROOT_ACCOUNT_ID`: NEARウォレット設定
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
DATABASE_URL=postgres://postgres_test:postgres_test@localhost:5433/postgres_test cargo test -- --nocapture
```

### データベースマイグレーション
- データベーススキーマ変更にはDieselを使用
- `diesel migration run` でマイグレーションを実行
- `diesel migration generate <name>` で新しいマイグレーションを作成
- スキーマは `crates/backend/src/persistence/schema.rs` で定義