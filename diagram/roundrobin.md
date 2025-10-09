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

```
backend/src/jsonrpc/
├── mod.rs                    # 既存: JsonRpcClient の定義
├── rpc.rs                    # 既存: RPCメソッド実装
├── endpoint_pool.rs          # 新規: エンドポイント選択・管理
└── endpoint_pool/
    ├── mod.rs                # EndpointPool の公開API
    ├── selector.rs           # Weighted random selection ロジック
    ├── failure_tracker.rs    # 失敗エンドポイント追跡
    └── config.rs             # 環境変数パース
```

### 1. EndpointSelector trait（テスト境界）

```rust
// backend/src/jsonrpc/endpoint_pool/selector.rs

/// エンドポイント選択の抽象化（モック可能）
pub trait EndpointSelector: Send + Sync {
    /// 利用可能なエンドポイントから1つ選択
    fn select<'a>(&self, available: &'a [RpcEndpoint]) -> Option<&'a RpcEndpoint>;
}

/// Weighted random selection の実装
pub struct WeightedRandomSelector;

impl EndpointSelector for WeightedRandomSelector {
    fn select<'a>(&self, available: &'a [RpcEndpoint]) -> Option<&'a RpcEndpoint> {
        if available.is_empty() {
            return None;
        }

        let total_weight: u32 = available.iter().map(|ep| ep.weight).sum();

        if total_weight == 0 {
            // 均等ランダム
            let mut rng = rand::thread_rng();
            let idx = rng.gen_range(0..available.len());
            return Some(&available[idx]);
        }

        // 重み付きランダム
        let mut rng = rand::thread_rng();
        let mut random_weight = rng.gen_range(0..total_weight);

        for endpoint in available {
            if random_weight < endpoint.weight {
                return Some(endpoint);
            }
            random_weight -= endpoint.weight;
        }

        available.first()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_selection_distribution() {
        let selector = WeightedRandomSelector;
        let endpoints = vec![
            RpcEndpoint { url: "a".into(), weight: 70, max_retries: 3 },
            RpcEndpoint { url: "b".into(), weight: 30, max_retries: 3 },
        ];

        // 1000回試行して分布を確認
        let mut count_a = 0;
        for _ in 0..1000 {
            let selected = selector.select(&endpoints).unwrap();
            if selected.url == "a" {
                count_a += 1;
            }
        }

        // 70%前後になることを確認（600-800の範囲）
        assert!(count_a > 600 && count_a < 800);
    }

    #[test]
    fn test_equal_weight_selection() {
        let selector = WeightedRandomSelector;
        let endpoints = vec![
            RpcEndpoint { url: "a".into(), weight: 0, max_retries: 3 },
            RpcEndpoint { url: "b".into(), weight: 0, max_retries: 3 },
        ];

        // weight=0 でも均等選択される
        let selected = selector.select(&endpoints);
        assert!(selected.is_some());
    }

    #[test]
    fn test_empty_endpoints() {
        let selector = WeightedRandomSelector;
        let endpoints: Vec<RpcEndpoint> = vec![];
        assert!(selector.select(&endpoints).is_none());
    }
}
```

### 2. FailureTracker trait（時刻注入）

```rust
// backend/src/jsonrpc/endpoint_pool/failure_tracker.rs

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// 時刻取得の抽象化（テスト時にモック可能）
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

/// 本番環境用の実装
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// 失敗エンドポイントの追跡
pub struct FailureTracker {
    failed_until: Arc<RwLock<HashMap<String, Instant>>>,
    reset_duration: Duration,
    clock: Arc<dyn Clock>,
}

impl FailureTracker {
    pub fn new(reset_duration: Duration, clock: Arc<dyn Clock>) -> Self {
        Self {
            failed_until: Arc::new(RwLock::new(HashMap::new())),
            reset_duration,
            clock,
        }
    }

    /// エンドポイントが失敗中かチェック
    pub fn is_failed(&self, url: &str) -> bool {
        let failed = self.failed_until.read().unwrap();
        if let Some(&until) = failed.get(url) {
            self.clock.now() < until
        } else {
            false
        }
    }

    /// エンドポイントを失敗としてマーク
    pub fn mark_failed(&self, url: &str) {
        let until = self.clock.now() + self.reset_duration;
        self.failed_until.write().unwrap().insert(url.to_string(), until);

        warn!(log, "endpoint marked as failed";
            "url" => url,
            "reset_after_seconds" => self.reset_duration.as_secs()
        );
    }

    /// 失敗状態を手動でクリア（テスト用）
    #[cfg(test)]
    pub fn clear(&self) {
        self.failed_until.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// テスト用のモック時計
    struct MockClock {
        now: Mutex<Instant>,
    }

    impl MockClock {
        fn new() -> Self {
            Self {
                now: Mutex::new(Instant::now()),
            }
        }

        fn advance(&self, duration: Duration) {
            *self.now.lock().unwrap() += duration;
        }
    }

    impl Clock for MockClock {
        fn now(&self) -> Instant {
            *self.now.lock().unwrap()
        }
    }

    #[test]
    fn test_failure_tracking() {
        let clock = Arc::new(MockClock::new());
        let tracker = FailureTracker::new(
            Duration::from_secs(300),
            clock.clone() as Arc<dyn Clock>,
        );

        // 初期状態
        assert!(!tracker.is_failed("test.url"));

        // 失敗マーク
        tracker.mark_failed("test.url");
        assert!(tracker.is_failed("test.url"));

        // 時間を進める（300秒未満）
        clock.advance(Duration::from_secs(200));
        assert!(tracker.is_failed("test.url"));

        // 時間を進める（300秒経過）
        clock.advance(Duration::from_secs(101));
        assert!(!tracker.is_failed("test.url"));
    }
}
```

### 3. EndpointPool の統合

```rust
// backend/src/jsonrpc/endpoint_pool/mod.rs

use super::selector::{EndpointSelector, WeightedRandomSelector};
use super::failure_tracker::{FailureTracker, SystemClock};
use super::config::load_endpoints_from_env;

pub struct EndpointPool {
    endpoints: Vec<RpcEndpoint>,
    selector: Box<dyn EndpointSelector>,
    failure_tracker: FailureTracker,
}

impl EndpointPool {
    /// 本番環境用のコンストラクタ
    pub fn new() -> Self {
        let endpoints = load_endpoints_from_env();
        let selector = Box::new(WeightedRandomSelector);
        let failure_tracker = FailureTracker::new(
            Duration::from_secs(300),
            Arc::new(SystemClock),
        );

        Self {
            endpoints,
            selector,
            failure_tracker,
        }
    }

    /// テスト用のコンストラクタ（依存性注入）
    #[cfg(test)]
    pub fn with_dependencies(
        endpoints: Vec<RpcEndpoint>,
        selector: Box<dyn EndpointSelector>,
        failure_tracker: FailureTracker,
    ) -> Self {
        Self {
            endpoints,
            selector,
            failure_tracker,
        }
    }

    pub fn next_endpoint(&self) -> Option<&RpcEndpoint> {
        // 利用可能なエンドポイントをフィルタ
        let available: Vec<_> = self
            .endpoints
            .iter()
            .filter(|ep| !self.failure_tracker.is_failed(&ep.url))
            .collect();

        if available.is_empty() {
            warn!(log, "all endpoints failed, retrying all");
            // 全失敗時は全エンドポイントを再試行
            return self.selector.select(&self.endpoints);
        }

        self.selector.select(&available)
    }

    pub fn mark_failed(&self, url: &str) {
        self.failure_tracker.mark_failed(url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSelector {
        next_index: std::sync::Mutex<usize>,
    }

    impl EndpointSelector for MockSelector {
        fn select<'a>(&self, available: &'a [RpcEndpoint]) -> Option<&'a RpcEndpoint> {
            let mut idx = self.next_index.lock().unwrap();
            let result = available.get(*idx);
            *idx = (*idx + 1) % available.len().max(1);
            result
        }
    }

    #[test]
    fn test_endpoint_pool_basic() {
        let endpoints = vec![
            RpcEndpoint { url: "a".into(), weight: 50, max_retries: 3 },
            RpcEndpoint { url: "b".into(), weight: 50, max_retries: 3 },
        ];

        let pool = EndpointPool::with_dependencies(
            endpoints,
            Box::new(MockSelector { next_index: Mutex::new(0) }),
            FailureTracker::new(Duration::from_secs(300), Arc::new(SystemClock)),
        );

        // 最初は "a" が選ばれる
        assert_eq!(pool.next_endpoint().unwrap().url, "a");

        // "a" を失敗マーク → "b" が選ばれる
        pool.mark_failed("a");
        assert_eq!(pool.next_endpoint().unwrap().url, "b");
    }
}
```

### 4. 設定の環境変数パース

```rust
// backend/src/jsonrpc/endpoint_pool/config.rs

use std::env;

pub fn load_endpoints_from_env() -> Vec<RpcEndpoint> {
    // 後方互換: NEAR_RPC_URL が設定されていれば単一エンドポイント
    if let Ok(url) = env::var("NEAR_RPC_URL") {
        return vec![RpcEndpoint {
            url,
            weight: 100,
            max_retries: 5,
        }];
    }

    // 新形式: カンマ区切りのエンドポイント
    let urls = env::var("NEAR_RPC_ENDPOINTS")
        .unwrap_or_else(|_| default_endpoints_string());

    let weights = env::var("NEAR_RPC_WEIGHTS")
        .unwrap_or_else(|_| "40,40,15,5".to_string());

    parse_endpoints(&urls, &weights)
}

fn default_endpoints_string() -> String {
    "https://rpc.ankr.com/near,https://near.drpc.org,https://free.rpc.fastnear.com,https://1rpc.io/near"
        .to_string()
}

fn parse_endpoints(urls: &str, weights: &str) -> Vec<RpcEndpoint> {
    let url_list: Vec<&str> = urls.split(',').collect();
    let weight_list: Vec<u32> = weights
        .split(',')
        .filter_map(|w| w.trim().parse().ok())
        .collect();

    url_list
        .into_iter()
        .enumerate()
        .map(|(i, url)| RpcEndpoint {
            url: url.trim().to_string(),
            weight: weight_list.get(i).copied().unwrap_or(10),
            max_retries: 3,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_endpoints() {
        let urls = "http://a,http://b";
        let weights = "70,30";
        let endpoints = parse_endpoints(urls, weights);

        assert_eq!(endpoints.len(), 2);
        assert_eq!(endpoints[0].url, "http://a");
        assert_eq!(endpoints[0].weight, 70);
        assert_eq!(endpoints[1].url, "http://b");
        assert_eq!(endpoints[1].weight, 30);
    }

    #[test]
    fn test_parse_endpoints_missing_weights() {
        let urls = "http://a,http://b,http://c";
        let weights = "70";
        let endpoints = parse_endpoints(urls, weights);

        assert_eq!(endpoints.len(), 3);
        assert_eq!(endpoints[0].weight, 70);
        assert_eq!(endpoints[1].weight, 10); // デフォルト
        assert_eq!(endpoints[2].weight, 10); // デフォルト
    }
}
```

### テスト戦略

#### 単体テスト
1. **selector.rs**: ランダム選択の分布テスト
2. **failure_tracker.rs**: 時刻依存処理のモックテスト
3. **config.rs**: 環境変数パースのエッジケース

#### 統合テスト
```rust
// backend/tests/endpoint_pool_integration_test.rs

#[tokio::test]
async fn test_endpoint_failover() {
    // モックRPCサーバーを立てて実際のフェイルオーバーをテスト
    let mock_server_a = MockServer::start().await;
    let mock_server_b = MockServer::start().await;

    // サーバーAは rate limit を返す
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&mock_server_a)
        .await;

    // サーバーBは成功を返す
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "result": "ok"
        })))
        .mount(&mock_server_b)
        .await;

    // EndpointPoolを設定
    env::set_var("NEAR_RPC_ENDPOINTS", format!("{},{}",
        mock_server_a.uri(), mock_server_b.uri()));

    // リクエスト実行
    let result = call_with_fallback(method).await;

    // サーバーBにフォールバックして成功
    assert!(result.is_ok());
}
```

### メリット

1. **テスタビリティ**:
   - 時刻・ランダム性を注入可能
   - モックサーバーで統合テスト可能

2. **保守性**:
   - 責務が分離されている
   - 各モジュールが独立してテスト可能

3. **拡張性**:
   - 新しいセレクター戦略を追加しやすい
   - メトリクス収集を後から追加可能

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
