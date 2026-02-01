use anyhow::Result;
use std::collections::HashMap;

/// Token decimals の取得を抽象化するトレイト
pub(crate) trait DecimalsFetcher {
    async fn fetch_decimals(&self, token_id: &str) -> Result<u8>;
}

/// BackendClient に DecimalsFetcher を実装
impl DecimalsFetcher for crate::api::backend::BackendClient {
    async fn fetch_decimals(&self, token_id: &str) -> Result<u8> {
        self.get_token_decimals(token_id).await
    }
}

/// Token decimals のローカルキャッシュ
pub(crate) struct TokenDecimalsCache {
    cache: HashMap<String, u8>,
}

impl TokenDecimalsCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// キャッシュから取得。なければ fetcher 経由で取得してキャッシュ
    pub async fn resolve(&mut self, fetcher: &impl DecimalsFetcher, token_id: &str) -> Result<u8> {
        if let Some(&d) = self.cache.get(token_id) {
            return Ok(d);
        }
        let d = fetcher.fetch_decimals(token_id).await?;
        self.cache.insert(token_id.to_string(), d);
        Ok(d)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct MockFetcher {
        responses: HashMap<String, u8>,
        call_count: Cell<usize>,
    }

    impl MockFetcher {
        fn new(responses: Vec<(&str, u8)>) -> Self {
            Self {
                responses: responses
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
                call_count: Cell::new(0),
            }
        }

        fn call_count(&self) -> usize {
            self.call_count.get()
        }
    }

    impl DecimalsFetcher for MockFetcher {
        async fn fetch_decimals(&self, token_id: &str) -> Result<u8> {
            self.call_count.set(self.call_count.get() + 1);
            self.responses
                .get(token_id)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("token not found: {}", token_id))
        }
    }

    #[tokio::test]
    async fn test_cache_hit_avoids_refetch() {
        let fetcher = MockFetcher::new(vec![("token_a", 18)]);
        let mut cache = TokenDecimalsCache::new();

        let d1 = cache.resolve(&fetcher, "token_a").await.unwrap();
        let d2 = cache.resolve(&fetcher, "token_a").await.unwrap();

        assert_eq!(d1, 18);
        assert_eq!(d2, 18);
        assert_eq!(fetcher.call_count(), 1, "fetch should be called only once");
    }

    #[tokio::test]
    async fn test_cache_miss_fetches_and_stores() {
        let fetcher = MockFetcher::new(vec![("token_a", 18), ("token_b", 6)]);
        let mut cache = TokenDecimalsCache::new();

        let d_a = cache.resolve(&fetcher, "token_a").await.unwrap();
        let d_b = cache.resolve(&fetcher, "token_b").await.unwrap();

        assert_eq!(d_a, 18);
        assert_eq!(d_b, 6);
        assert_eq!(fetcher.call_count(), 2);
    }

    #[tokio::test]
    async fn test_fetch_error_not_cached() {
        let fetcher = MockFetcher::new(vec![]); // 空 → 全てエラー
        let mut cache = TokenDecimalsCache::new();

        let r1 = cache.resolve(&fetcher, "unknown").await;
        assert!(r1.is_err());

        // エラー後もキャッシュされていないことを確認
        let r2 = cache.resolve(&fetcher, "unknown").await;
        assert!(r2.is_err());
        assert_eq!(fetcher.call_count(), 2, "error should not be cached");
    }
}
