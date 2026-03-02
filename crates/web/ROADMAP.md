# Web API ロードマップ

gRPC (tonic) ベースの API サーバー。Slint クライアント（別リポジトリ、デスクトップ + モバイル）向け。

## 技術選定

クライアントが Rust (Slint) + 別リポジトリのため、proto ファイルによる型契約 + tonic-build を採用。

| 観点 | gRPC (tonic) ✓ | REST + 共有型クレート | REST + OpenAPI |
|------|:-:|:-:|:-:|
| API 契約 | .proto ファイル | 共有 Rust クレート (git dep) | OpenAPI spec |
| クライアント型生成 | tonic-build (idiomatic Rust) | 直接 Rust 型共有 | openapi-generator |
| ストリーミング | ネイティブ | SSE | SSE |
| モバイル互換 | tonic-web で HTTP/1.1 対応 | HTTP/1.1 | HTTP/1.1 |
| 既存コードとの親和性 | 変換層必要 | serde 型そのまま | serde 型そのまま |
| クライアント実装コスト | 最小 (生成済み) | reqwest 手書き | 生成コードが冗長 |

## Phase 1: Config API (現在)

- HealthService: DB 接続チェック付きヘルスチェック
- ConfigService: 設定の CRUD (GetAll, GetOne, Upsert, Delete)
  - `persistence::config_store` の既存関数を直接利用

## Phase 2: ポートフォリオ・取引閲覧

- PortfolioService
  - GetEvaluationPeriods: 評価期間一覧
  - GetEvaluationPeriod: 評価期間詳細
  - GetTrades: 取引履歴 (ページネーション)
  - GetTradesByBatch: バッチ単位の取引詳細
  - GetLatestBatch: 最新バッチ
  - GetLatestRates: 全トークン最新レート
  - GetRateHistory: レート履歴

利用する既存関数:
- `persistence::evaluation_period::EvaluationPeriod` (get_latest_async, etc.)
- `persistence::trade_transaction::TradeTransaction` (find_by_batch_id_async, get_latest_batch_id_async)
- `persistence::token_rate` (get_rates_in_time_range, etc.)

## Phase 3: アクション系 API

### Harvest

- HarvestService
  - Execute: 任意額ハーベスト実行
  - GetStatus: 最後のハーベスト状態

実装時の変更:
- `trade::harvest` に公開関数 `execute_harvest_with_amount()` を追加
- web クレートに `trade`, `blockchain` 依存を追加

### Simulation

- SimulationService
  - Start: シミュレーション開始 (server streaming で進捗配信)
  - GetResult: 完了済み結果取得

実装時の変更:
- `simulate` クレートを lib + bin 構成に分割
- `engine::run_simulation` に進捗コールバック追加
- web クレートに `simulate` 依存を追加

## Proto ファイル共有

Slint アプリ（別リポジトリ）との proto 共有方法:

1. proto ファイルはこのリポジトリの `crates/web/proto/` に配置
2. Slint リポジトリから git submodule として参照、または proto ファイルをコピー
3. 両側で `tonic-build` が同じ proto からコード生成

将来的に proto が増えたら独立リポジトリ `zaciraci-proto` に分離も可能。
