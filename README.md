[![codecov](https://codecov.io/github/fathens/zaciraci/graph/badge.svg?token=WJyG3oBkxH)](https://codecov.io/github/fathens/zaciraci)

開発要件については CONTRIBUTING.md を参照して下さい。

## Deployment

### Single-process invariant (CRITICAL)

`ensure_ref_storage_setup`（`crates/blockchain/src/ref_finance/storage.rs`）は
プロセスローカルの `REF_STORAGE_LOCKS`（`tokio::sync::Mutex` マップ）で
同一アカウントの並行実行を直列化している。**同じ `ROOT_ACCOUNT_ID` を握る backend を
複数プロセス/コンテナで同時起動することは禁止**（single-process 前提）。違反すると:

- 初期 `storage_deposit` が二重実行され、`bounds.min` 相当の NEAR が余計に
  `account.storage` に積まれる（refund は stale view に依存するため保証されない）。
- top-up が並行発火して per-call cap が壊れ、理論上 `max_top_up × 同時プロセス数`
  まで単一サイクルで流出しうる（cap 会計は process-local）。

クロスプロセス排他（`persistence::pg_advisory_lock` 連携 / `trait CrossProcessLock` 導入）は
follow-up Issue #1 (P0) で対応予定。それまでは以下の orchestrator 別ガードを必ず設定すること。

### Orchestrator 別の設定

| Orchestrator | ガード内容 |
|---|---|
| **fly.io**（本リポ想定） | `fly.toml` の `[[services]]` に `min_machines_running = 1` を維持し、`auto_start_machines = false` / `auto_stop_machines = false` を徹底。追加 machine を立てる運用は厳禁。 |
| **Kubernetes** | `Deployment.strategy.type = "Recreate"` + `replicas: 1` を推奨（rolling update で 2 Pod 並ぶ期間を作らない）。`Deployment` を避ける場合は `StatefulSet` + `replicas: 1` + `podManagementPolicy: OrderedReady` + `updateStrategy.type: OnDelete` で逐次更新を強制する。 |
| **docker-compose** | `deploy.replicas: 1` + `deploy.update_config.order: stop-first`。 |
| **systemd / supervisord** | single-instance `.service` + `PIDFile=/var/run/zaciraci-${ROOT_ACCOUNT_ID}.pid` で排他起動を強制。 |

### 多重起動時に壊れる不変条件

- `initial_deposit ≤ max_top_up`（`storage.rs` 初期 deposit cap guard の strict `>` 判定）
- `actual_top_up + initial_deposit ≤ max_top_up`（`handle_normal_plan` の `remaining_cap` 算出）
- いずれもプロセスローカルで成立するため、複数プロセスから呼ばれると実効 cap は
  `max_top_up × プロセス数` に劣化する。

### Mainnet dry-run

新しい config や新しい wallet を mainnet に投入する際は、事前に以下を実施する:

1. `ref_storage_max_top_up_yoctonear` を **`bounds.min × 1.1` 以上** の小さめの値
   （例: `1_500_000_000_000_000_000_000` = 0.0015 NEAR。現行 `bounds.min ≈ 1.25e21`
   に対し約 1.2x の余裕）に縮小した `run_*/docker-compose.yml` で 1 サイクル起動する。
   `bounds.min` そのもの（= 0.001 NEAR）だと step 1 cap guard の strict `>` 境界に即触れ、
   needed_tokens ≥ 1 で dry-run が即エラーになるため避ける。
2. 期待 top-up 額 = `bounds.min × needed_tokens × 1.1` を事前計算し、
   観測値（`ref storage top-up` warn ログの `amount` フィールド）と突き合わせる。
3. dry-run が失敗した場合、REF Finance から手動で `storage_withdraw` を発行し、
   誤って積まれた NEAR を回収する。**必ず `--deposit 1 yoctoNEAR` を付与**すること
   （REF 契約の `assert_one_yocto` 要件）。発行前に `crates/blockchain/src/ref_finance/contract_spec.md` §2.3
   の自動 unregister 条件を確認し、意図せぬ `register_tokens` 再発動を避ける。
4. dry-run 成功を確認してから本来の `max_top_up` 値に戻して本番投入。

### REF 契約 hash 監視

`Plan::InitialRegister` 経路の安全性（`actual_top_up = 0` を型レベルで保証）は、
REF exchange コントラクトの `register_tokens` が `attached_deposit = 1 yocto` 仕様を
維持することに依存する。契約 upgrade を見逃さないため:

- 週次で REF exchange (`v2.ref-finance.near`) の WASM hash を取得して記録する。
  手順は NEAR RPC の `query { request_type: "view_code", account_id: "v2.ref-finance.near" }`
  で WASM バイナリを取得し sha256 を計算する（`near-cli-rs` の
  `near contract download-wasm v2.ref-finance.near ...` 等でも可）。
- hash 変化を検知したら `MAX_REGISTER_PER_CYCLE` と `per_token_floor`（= `bounds.min`）の
  再評価を行い、必要に応じ `crates/blockchain/src/ref_finance/storage/planner.rs` 冒頭の
  倍率テーブルを更新する。

### 運用時の曝露想定（参考値）

現行 default (`max_top_up = 0.5 NEAR`) を前提に、1 ウォレット 1 プロセス運用で
`ensure_ref_storage_setup` を 10 cycles/日 程度動かす場合、最大曝露は
`max_top_up × cycles/日 × wallets = 5 NEAR/日/wallet` 程度。
連続失敗時は `max_top_up × 失敗回数` まで伸びうるため、follow-up Issue #2
（呼び出し側 retry 上限）と Issue #3（累積 top-up monitoring）で検知・抑制する。

### Alert 閾値の由来

具体的な閾値設定は follow-up Issue #3 の monitoring 構成で管理する。閾値導出の根拠のみ以下に示す:

- **warn**: `cumulative_top_up_daily` が期待曝露（上記例示値）を越えた時点で通知。
  eng oncall 宛。
- **critical**: `max_top_up × 10` 相当（通常運用では到達しない水準）を越えたら finance 通知。
- **cap breach**: `actual_top_up > remaining_cap` による `Err` が観測されたら security 通知（cap-bypass の前兆）。
