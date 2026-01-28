# jsonrpc モジュール

## 概要
NEAR Protocol JSON-RPC クライアントの実装。

## コンポーネント
- `endpoint_pool.rs` - エンドポイント管理
- `rpc.rs` - RPC クライアント実装
- `near_client.rs` - NEAR 固有の操作
- `sent_tx.rs` - トランザクション送信管理

## EndpointPool
重み付きランダム選択と障害追跡機能を持つ RPC エンドポイント管理。

### リトライ戦略
1. エンドポイント固定リトライ: 各エンドポイントに対して max_retries 回リトライ
2. エンドポイント切替: max_retries 到達後、mark_failed() して次のエンドポイントへ
3. グローバル制限: 全体で retry_limit 回を超えたらエラー返却

### 設定例 (zaciraci.toml)
```toml
[[rpc.endpoints]]
url = "https://rpc.fastnear.com"
weight = 100
max_retries = 3
```

### 挙動詳細
| ケース | 挙動 |
|--------|------|
| max_retries = 0 | 初回失敗で即座に次のエンドポイントへ |
| TooManyRequests | 即座に mark_failed() して次のエンドポイントへ |
| 全エンドポイント失敗 | failures をリセットして再試行 |
