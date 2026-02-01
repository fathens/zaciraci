# NEAR RPC エンドポイント ラウンドロビン実装計画

## 目的

複数の無料RPCエンドポイントをラウンドロビン方式で使用し、以下を実現する:

1. **Rate Limit回避**: 単一エンドポイントへの集中を防ぐ
2. **可用性向上**: 1つのエンドポイントが障害でも継続稼働
3. **コスト最適化**: 無料プランを最大限活用

## 選定エンドポイント

### 採用候補（無料プラン）

| プロバイダー | エンドポイント | Rate Limit | 月間クォータ | 優先度 |
|------------|--------------|-----------|------------|-------|
| **Ankr** | `https://rpc.ankr.com/near` | 30 req/s | 200M Credits | 高 |
| **dRPC** | `https://near.drpc.org` | 120,000 CU/分 | 210M CU | 高 |
| **FASTNEAR** | `https://free.rpc.fastnear.com` | 不明 | 不明 | 中 |
| **1RPC** | `https://1rpc.io/near` | 日次制限 | 不明 | 中 |
| **BlockPI** | `https://near.blockpi.network/v1/rpc/public` | 10 req/s | 50M RUs | 低 |

### 選定基準

**優先度 高**:
- Rate limitが明確
- 30 req/s以上
- ドキュメントが充実

**優先度 中**:
- Rate limit不明だが実績あり
- 高速を謳っている

**優先度 低**:
- Rate limitが低すぎる（10 req/s）
- バックアップとしてのみ使用

## アーキテクチャ設計

### 1. エンドポイント設定

```rust
// backend/src/jsonrpc.rs

pub struct RpcEndpoint {
    url: String,
    weight: u32,        // ランダム選択の重み（リクエスト配分比率: 40 = 40%の確率で選択）
    max_retries: u32,   // このエンドポイントでの最大リトライ回数
}

static RPC_ENDPOINTS: Lazy<Vec<RpcEndpoint>> = Lazy::new(|| {
    vec![
        RpcEndpoint {
            url: "https://rpc.ankr.com/near".to_string(),
            weight: 40,
            max_retries: 3,
        },
        RpcEndpoint {
            url: "https://near.drpc.org".to_string(),
            weight: 40,
            max_retries: 3,
        },
        RpcEndpoint {
            url: "https://free.rpc.fastnear.com".to_string(),
            weight: 15,
            max_retries: 2,
        },
        RpcEndpoint {
            url: "https://1rpc.io/near".to_string(),
            weight: 5,
            max_retries: 2,
        },
    ]
});
```

### 2. ウェイトベースランダム選択戦略

#### Weighted Random Selection

```
リクエスト配分例（weight基準の期待値）:
- Ankr: 40% (30 req/s limit)
- dRPC: 40% (120,000 CU/分 ≈ 2,000 CU/s)
- FASTNEAR: 15%
- 1RPC: 5%
```

**アルゴリズム**:
1. 利用可能なエンドポイントから重みに基づいてランダム選択
2. リクエスト実行
3. 成功 → 完了
4. 失敗（rate limit）→ そのエンドポイントを一時的に無効化して別のエンドポイントで再試行
5. max_retries到達 → エラー返却

**ラウンドロビンではなくランダムにする理由**:
- ✅ **負荷分散が自然**: 長期的に重み通りに分散される
- ✅ **実装がシンプル**: インデックス管理不要
- ✅ **並行処理に強い**: 複数スレッドから同時呼び出しでも問題なし
- ✅ **偏りが少ない**: 連続リクエストでも異なるエンドポイントが選ばれる可能性

### 3. フェイルオーバー機構

```
next_endpoint():
  available = endpoints.filter(not failed)
  if available.empty():
    reset all failures
    return first endpoint
  return weighted_random_select(available)

mark_failed(url):
  failed_endpoints.insert(url, until: now + 5min)
  log warning
  schedule auto-reset after 5min
```

### 4. リトライロジック

```
call_with_fallback(method):
  for attempt in 0..MAX_ATTEMPTS:
    endpoint = pool.next_endpoint()
    client = connect(endpoint.url)

    match call(client, method):
      Ok(response) -> return response
      Err(RateLimitError) -> pool.mark_failed(endpoint.url); continue
      Err(e) -> return e

  return MaxAttemptsExceeded
```

## テスト可能な構造設計

### アーキテクチャ方針

**依存性注入パターンを採用**:
- エンドポイント選択ロジックを独立したモジュールに分離
- trait を使ってモック可能な設計
- 時刻依存処理（失敗リセット）をテスタブルに

### モジュール構成

```
backend/src/
├── jsonrpc.rs                          # 親モジュール（mod宣言 + 公開API）
└── jsonrpc/
    ├── near_client.rs                  # 既存: NEAR特化クライアント
    ├── rpc.rs                          # 既存: RPCメソッド実装
    ├── sent_tx.rs                      # 既存: トランザクション送信
    ├── endpoint_pool.rs                # 新規: EndpointPool（公開API + 統合）
    └── endpoint_pool/
        ├── selector.rs                 # Weighted random selection
        ├── selector/
        │   └── tests.rs                # selector のテスト
        ├── failure.rs                  # 失敗エンドポイント追跡
        ├── failure/
        │   └── tests.rs                # failure のテスト
        ├── config.rs                   # 環境変数パース
        ├── config/
        │   └── tests.rs                # config のテスト
        └── tests.rs                    # endpoint_pool 統合テスト
```

**参考にしたパターン**:
- `ref_finance/path/graph.rs` + `graph/tests.rs`
- `web/pools/sort.rs` + `sort/tests.rs`
- テストファイルは `#[cfg(test)] mod tests;` で参照

**ファイル分割方針**:
- `endpoint_pool.rs`: 公開API（EndpointPool struct）のみ
- `endpoint_pool/*.rs`: 各責務を独立したファイルに分割
- テストも各ファイル内に配置

### ファイル構成

**backend/src/jsonrpc.rs**:
```rust
mod endpoint_pool;
// ... 既存のmod宣言
```

**backend/src/jsonrpc/endpoint_pool.rs**:
```rust
mod selector;
mod failure;
mod config;

use selector::{EndpointSelector, WeightedRandomSelector};
use failure::{FailureTracker, SystemClock};
use config::load_endpoints_from_env;

pub struct RpcEndpoint { url, weight, max_retries }
pub struct EndpointPool { ... }

impl EndpointPool {
    pub fn new() -> Self { ... }
    pub fn next_endpoint(&self) -> Option<&RpcEndpoint> { ... }
    pub fn mark_failed(&self, url: &str) { ... }
}

#[cfg(test)]
mod tests;  // endpoint_pool/tests.rs を参照
```

**backend/src/jsonrpc/endpoint_pool/selector.rs**:
```rust
pub trait EndpointSelector: Send + Sync { ... }
pub struct WeightedRandomSelector;

#[cfg(test)]
mod tests;  // selector/tests.rs を参照
```

**backend/src/jsonrpc/endpoint_pool/failure.rs**:
```rust
pub trait Clock: Send + Sync { ... }
pub struct SystemClock;
pub struct FailureTracker { ... }

#[cfg(test)]
mod tests;  // failure/tests.rs を参照
```

**backend/src/jsonrpc/endpoint_pool/config.rs**:
```rust
pub fn load_endpoints_from_env() -> Vec<RpcEndpoint> { ... }

#[cfg(test)]
mod tests;  // config/tests.rs を参照
```

### 主要コンポーネント

**EndpointPool**:
- `new()`: 環境変数から設定をロード
- `next_endpoint()`: 利用可能なエンドポイントを重み付きランダム選択
- `mark_failed()`: エンドポイントを一時的に無効化（5分間）

**selector::EndpointSelector trait**:
- `WeightedRandomSelector`: 重み付きランダム選択の実装
- テスト用のモック実装が可能

**failure::FailureTracker**:
- `Clock trait`: 時刻取得の抽象化（テスト用MockClock実装）
- 失敗エンドポイントを時刻ベースで管理

**config モジュール**:
- TOML設定ファイル (`config/config.toml`) から `rpc.endpoints` をロード
- 環境変数でのオーバーライドも可能（後方互換）

## 実装手順

### Phase 1: 基礎実装（1-2時間）

1. **エンドポイント設定構造の追加**
   - `backend/src/jsonrpc.rs` に `RpcEndpoint` struct 追加
   - 環境変数 `NEAR_RPC_ENDPOINTS` でカスタマイズ可能に
   - デフォルトは上記4エンドポイント

2. **EndpointPool の実装**
   - `backend/src/jsonrpc/endpoint_pool.rs` 新規作成
   - Weighted Round Robin ロジック
   - Failed endpoint tracking

3. **既存コードとの統合**
   - `new_client()` を修正してEndpointPoolを使用
   - `JSONRPC_CLIENT` の初期化ロジック変更

### Phase 2: リトライ改善（1時間）

1. **Rate limit検出の強化**
   - `is_rate_limit_error()` 関数の実装
   - HTTP 429 および "too many requests" 文字列検出

2. **エンドポイント切り替えロジック**
   - Rate limit時に即座に次のエンドポイントへ
   - 一時的な無効化機構

3. **リトライバグ修正**（未実装項目2.2から）
   - `jsonrpc/rpc.rs:226` の `.min(min_dur)` → `.max(min_dur)` 修正

### Phase 3: 監視とロギング（30分）

1. **メトリクス収集**
   - エンドポイントごとのリクエスト数
   - 成功率
   - Rate limit到達回数

2. **詳細ログ**
   ```rust
   info!(log, "RPC endpoint selected";
       "url" => endpoint.url,
       "weight" => endpoint.weight,
       "attempt" => attempt_count
   );

   warn!(log, "endpoint marked as failed";
       "url" => endpoint.url,
       "reason" => "rate_limit",
       "retry_after" => failure_reset_interval
   );
   ```

### Phase 4: テストと検証（1-2時間）

1. **単体テスト**
   - EndpointPool のロジック
   - Weighted selection
   - Failure tracking

2. **統合テスト**
   - 実際のRPC呼び出しで動作確認
   - Rate limit発生時の挙動

3. **本番検証**
   - 次回cron実行で動作確認
   - ログでエンドポイント切り替えを確認

## 環境変数設定

### 新規追加

```bash
# run_local/.env

# カンマ区切りで複数エンドポイント指定
export NEAR_RPC_ENDPOINTS="https://rpc.ankr.com/near,https://near.drpc.org,https://free.rpc.fastnear.com"

# エンドポイントの重み（カンマ区切り、順序は上記と対応）
export NEAR_RPC_WEIGHTS="40,40,20"

# 失敗エンドポイントのリセット間隔（秒）
export NEAR_RPC_FAILURE_RESET_SECONDS="300"  # 5分

# 全エンドポイント試行の最大回数
export NEAR_RPC_MAX_ENDPOINT_ATTEMPTS="10"
```


## 期待効果

### Rate Limit回避

**現状**（単一エンドポイント）:
- `rpc.mainnet.near.org`: 7分でrate limit到達
- 100+ RPCリクエスト → 全て同じエンドポイント

**改善後**（4エンドポイント）:
- Ankr: 40%のリクエスト → 30 req/s limitに余裕
- dRPC: 40%のリクエスト → 120,000 CU/分に余裕
- FASTNEAR: 15%
- 1RPC: 5%

**試算**:
- 100リクエストを4エンドポイントで分散
- Ankr: 40リクエスト（1.3秒以内）
- dRPC: 40リクエスト（即座）
- FASTNEAR: 15リクエスト（不明だが高速）
- 1RPC: 5リクエスト（日次制限内）

→ **全体で2-3秒以内に完了**（現状は7分以上）

### 可用性向上

- 1つのエンドポイント障害でも継続稼働
- 自動フェイルオーバー
- 5分後に自動復帰

### コスト最適化

- 全て無料プランで運用可能
- 各プロバイダーの無料枠を最大活用
- 有料プラン不要

## リスクと対策

### リスク1: エンドポイント間の一貫性

**問題**: 各エンドポイントで同期タイミングが異なる可能性

**対策**:
- Finality指定で最終確定済みデータのみ取得
- トランザクション送信は単一エンドポイントで完結
- 読み取りのみラウンドロビン

### リスク2: デバッグの複雑化

**問題**: どのエンドポイントでエラーが発生したか追跡困難

**対策**:
- 全ログにエンドポイントURL記録
- エンドポイント別のメトリクス収集
- トランザクションハッシュと使用エンドポイントの紐付け

### リスク3: 無料プランの突然の変更

**問題**: プロバイダーがrate limitを変更する可能性

**対策**:
- 環境変数で簡単に設定変更可能
- 複数エンドポイント保持で影響を分散
- 定期的な動作確認

## モニタリング指標

### 実装すべきメトリクス

1. **エンドポイント使用率**
   - 各エンドポイントへのリクエスト数
   - 成功/失敗の比率

2. **Rate Limit到達**
   - 各エンドポイントでのrate limit発生回数
   - 無効化された回数と期間

3. **レスポンス時間**
   - エンドポイント別の平均レスポンスタイム
   - 最遅エンドポイントの特定

4. **フェイルオーバー**
   - フェイルオーバー発生回数
   - フェイルオーバー後の成功率

## 参考資料

- endpoints.md: 各エンドポイントの詳細調査結果
- backend/src/jsonrpc/rpc.rs: 既存のリトライロジック
- plan.md: 全体の実装計画

## 実装スケジュール

- **Phase 1**: 2-3時間（基礎実装）
- **Phase 2**: 1時間（リトライ改善）
- **Phase 3**: 30分（ログ追加）
- **Phase 4**: 1-2時間（テスト）

**合計**: 4.5-6.5時間

**優先度**: 🔥 最優先（現在のrate limit問題の根本対策）
