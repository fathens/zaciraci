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

```rust
use rand::Rng;

pub struct EndpointPool {
    endpoints: Vec<RpcEndpoint>,
    failed_endpoints: Arc<RwLock<HashSet<String>>>,  // 一時的に無効化されたエンドポイント
    failure_reset_interval: Duration,  // 無効化解除までの時間（例: 5分）
}

impl EndpointPool {
    pub fn next_endpoint(&self) -> Option<&RpcEndpoint> {
        let failed = self.failed_endpoints.read().unwrap();

        // 利用可能なエンドポイントのみをフィルタ
        let available: Vec<_> = self.endpoints
            .iter()
            .filter(|ep| !failed.contains(&ep.url))
            .collect();

        if available.is_empty() {
            // 全エンドポイント失敗 → リセット
            drop(failed);
            self.failed_endpoints.write().unwrap().clear();
            warn!(log, "all endpoints failed, resetting failed list");
            return self.endpoints.first();
        }

        // Weighted Random Selection で選択
        self.select_by_weight_random(&available)
    }

    fn select_by_weight_random(&self, endpoints: &[&RpcEndpoint]) -> Option<&RpcEndpoint> {
        // 重みの合計を計算
        let total_weight: u32 = endpoints.iter().map(|ep| ep.weight).sum();

        if total_weight == 0 {
            // 全ての重みが0の場合は均等にランダム選択
            let mut rng = rand::thread_rng();
            let idx = rng.gen_range(0..endpoints.len());
            return Some(endpoints[idx]);
        }

        // 重みに基づいてランダム選択
        let mut rng = rand::thread_rng();
        let mut random_weight = rng.gen_range(0..total_weight);

        for endpoint in endpoints {
            if random_weight < endpoint.weight {
                return Some(endpoint);
            }
            random_weight -= endpoint.weight;
        }

        // フォールバック（通常は到達しない）
        endpoints.first().copied()
    }

    pub fn mark_failed(&self, url: &str) {
        self.failed_endpoints.write().unwrap().insert(url.to_string());

        warn!(log, "endpoint marked as failed";
            "url" => url,
            "reset_after_seconds" => self.failure_reset_interval.as_secs()
        );

        // 一定時間後に自動解除
        let failed_eps = Arc::clone(&self.failed_endpoints);
        let url = url.to_string();
        let interval = self.failure_reset_interval;

        tokio::spawn(async move {
            tokio::time::sleep(interval).await;
            failed_eps.write().unwrap().remove(&url);
            info!(log, "endpoint failure reset"; "url" => url);
        });
    }
}
```

### 4. リトライロジックの改善

現在の `jsonrpc/rpc.rs` のリトライロジックを拡張:

```rust
// jsonrpc/rpc.rs

pub async fn call_with_fallback<M>(
    &self,
    method: M,
) -> MethodCallResult<M::Response, M::Error>
where
    M: methods::RpcMethod + Clone,
{
    let endpoint_pool = ENDPOINT_POOL.get_or_init(|| EndpointPool::new());

    for attempt in 0..MAX_ENDPOINT_ATTEMPTS {
        let endpoint = match endpoint_pool.next_endpoint() {
            Some(ep) => ep,
            None => return Err(RpcError::AllEndpointsFailed),
        };

        // エンドポイント固有のクライアントを作成
        let client = JsonRpcClient::connect(&endpoint.url);

        match self.call_single_endpoint(&client, method.clone()).await {
            Ok(response) => return Ok(response),
            Err(e) if is_rate_limit_error(&e) => {
                // Rate limit エラー → このエンドポイントを一時無効化
                endpoint_pool.mark_failed(&endpoint.url);
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(RpcError::MaxAttemptsExceeded)
}
```

## テスト可能な構造設計

### アーキテクチャ方針

**依存性注入パターンを採用**:
- エンドポイント選択ロジックを独立したモジュールに分離
- trait を使ってモック可能な設計
- 時刻依存処理（失敗リセット）をテスタブルに

### モジュール構成

既存コードのパターンに従った構成:

```
backend/src/
├── jsonrpc.rs                          # 親モジュール（mod宣言 + 公開API）
└── jsonrpc/
    ├── near_client.rs                  # 既存: NEAR特化クライアント
    ├── rpc.rs                          # 既存: RPCメソッド実装
    ├── sent_tx.rs                      # 既存: トランザクション送信
    └── endpoint_pool.rs                # 新規: EndpointPool（全機能統合）
```

**学習したパターン**:

1. **親モジュールファイル（jsonrpc.rs）**:
   - `mod サブモジュール名;` で子モジュールを宣言
   - 公開APIとtraitを定義
   - static変数とヘルパー関数を配置

2. **子モジュールファイル（jsonrpc/xxx.rs）**:
   - 単一の責務を持つ実装
   - 必要に応じてprivate submoduleを内部に持つ
   - テストも同じファイル内に `#[cfg(test)] mod tests`

3. **分割の基準**:
   - `ref_finance.rs`: 11個のサブモジュール（pub 8個, private 3個）
   - `ref_finance/path.rs`: 5個のサブモジュール（4個がprivate）
   - サブモジュールは機能単位で分割、共通部分は親で定義

**endpoint_pool.rs の設計方針**:

- **単一ファイルに統合**: selector, failure_tracker, config を全て含める
- **内部モジュール化**: `mod selector { ... }`, `mod failure { ... }`, `mod config { ... }`
- **公開API**: `pub struct EndpointPool` と必要な trait のみ公開
- **テスト**: 各内部モジュールに `#[cfg(test)] mod tests` を配置

この方針により:
- ファイル数を抑えつつ、論理的に分離
- 既存コードのスタイルに統一
- テストも同一ファイルで完結

### endpoint_pool.rs の構造

```
// backend/src/jsonrpc/endpoint_pool.rs

pub struct RpcEndpoint { url, weight, max_retries }
pub struct EndpointPool { ... }

// 内部モジュール
mod selector {
    pub trait EndpointSelector { ... }
    pub struct WeightedRandomSelector { ... }
    #[cfg(test)] mod tests { ... }
}

mod failure {
    pub trait Clock { ... }
    pub struct SystemClock { ... }
    pub struct FailureTracker { ... }
    #[cfg(test)] mod tests { ... }
}

mod config {
    pub fn load_endpoints_from_env() -> Vec<RpcEndpoint> { ... }
    #[cfg(test)] mod tests { ... }
}

#[cfg(test)]
mod tests {
    // EndpointPool の統合テスト
}
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
- 環境変数 `NEAR_RPC_ENDPOINTS`, `NEAR_RPC_WEIGHTS` をパース
- 後方互換: `NEAR_RPC_URL` も対応

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

### 既存設定との互換性

```rust
// 環境変数未設定時は単一エンドポイント（後方互換）
if let Ok(single_endpoint) = env::var("NEAR_RPC_URL") {
    // 従来の単一エンドポイントモード
    return vec![RpcEndpoint::new(single_endpoint, 100, 5, 1)];
}

// 新しい複数エンドポイントモード
parse_endpoints_from_env()
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
