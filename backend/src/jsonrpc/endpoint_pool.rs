use once_cell::sync::Lazy;
use rand::Rng;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use zaciraci_common::config;

/// RPC endpoint configuration
#[derive(Debug, Clone)]
pub struct RpcEndpoint {
    pub url: String,
    pub weight: u32,
    #[allow(dead_code)]
    pub max_retries: u32,
}

/// Manages multiple RPC endpoints with weighted random selection and failure tracking
pub struct EndpointPool {
    endpoints: Vec<RpcEndpoint>,
    failed_endpoints: Arc<Mutex<FailedEndpoints>>,
    #[allow(dead_code)]
    failure_reset_seconds: u64,
}

struct FailedEndpoints {
    failures: std::collections::HashMap<String, SystemTime>,
}

impl EndpointPool {
    /// Create a new EndpointPool from TOML configuration
    pub fn new() -> Self {
        let cfg = config::config();

        let mut endpoints: Vec<RpcEndpoint> = cfg
            .rpc
            .endpoints
            .iter()
            .map(|ep| RpcEndpoint {
                url: ep.url.clone(),
                weight: ep.weight,
                max_retries: ep.max_retries,
            })
            .collect();

        // If no endpoints in config, use defaults based on network
        if endpoints.is_empty() {
            let default_url = if cfg.network.use_mainnet {
                "https://rpc.mainnet.near.org"
            } else {
                "https://rpc.testnet.near.org"
            };
            endpoints.push(RpcEndpoint {
                url: default_url.to_string(),
                weight: 100,
                max_retries: 3,
            });
        }

        let failure_reset_seconds = cfg.rpc.settings.failure_reset_seconds;

        Self {
            endpoints,
            failed_endpoints: Arc::new(Mutex::new(FailedEndpoints {
                failures: std::collections::HashMap::new(),
            })),
            failure_reset_seconds,
        }
    }

    /// Select next available endpoint using weighted random selection
    pub fn next_endpoint(&self) -> Option<&RpcEndpoint> {
        let available = self.available_endpoints();
        if available.is_empty() {
            // Reset all failures if no endpoints available
            if let Ok(mut failed) = self.failed_endpoints.lock() {
                failed.failures.clear();
            }
            // Try again with first endpoint
            return self.endpoints.first();
        }

        self.weighted_random_select(&available)
    }

    /// Mark an endpoint as failed
    #[allow(dead_code)]
    pub fn mark_failed(&self, url: &str) {
        if let Ok(mut failed) = self.failed_endpoints.lock() {
            failed.failures.insert(
                url.to_string(),
                SystemTime::now() + Duration::from_secs(self.failure_reset_seconds),
            );
        }
    }

    /// Get list of currently available (non-failed) endpoints
    fn available_endpoints(&self) -> Vec<&RpcEndpoint> {
        let now = SystemTime::now();
        let failed = self.failed_endpoints.lock().ok();

        self.endpoints
            .iter()
            .filter(|ep| {
                if let Some(ref failed) = failed
                    && let Some(reset_time) = failed.failures.get(&ep.url)
                {
                    return now >= *reset_time;
                }
                true
            })
            .collect()
    }

    /// Weighted random selection from available endpoints
    fn weighted_random_select<'a>(
        &'a self,
        endpoints: &[&'a RpcEndpoint],
    ) -> Option<&'a RpcEndpoint> {
        if endpoints.is_empty() {
            return None;
        }

        let total_weight: u32 = endpoints.iter().map(|ep| ep.weight).sum();
        if total_weight == 0 {
            return endpoints.first().copied();
        }

        let mut rng = rand::rng();
        let mut random = rng.random_range(0..total_weight);

        for ep in endpoints {
            if random < ep.weight {
                return Some(ep);
            }
            random -= ep.weight;
        }

        endpoints.last().copied()
    }
}

impl Default for EndpointPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Global endpoint pool instance
pub static ENDPOINT_POOL: Lazy<EndpointPool> = Lazy::new(EndpointPool::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_pool_creation() {
        let pool = EndpointPool::new();
        assert!(
            !pool.endpoints.is_empty(),
            "Should load endpoints from config"
        );
    }

    #[test]
    fn test_weighted_random_select() {
        let endpoints = vec![
            RpcEndpoint {
                url: "http://test1".to_string(),
                weight: 50,
                max_retries: 3,
            },
            RpcEndpoint {
                url: "http://test2".to_string(),
                weight: 30,
                max_retries: 3,
            },
            RpcEndpoint {
                url: "http://test3".to_string(),
                weight: 20,
                max_retries: 3,
            },
        ];

        let pool = EndpointPool {
            endpoints: endpoints.clone(),
            failed_endpoints: Arc::new(Mutex::new(FailedEndpoints {
                failures: std::collections::HashMap::new(),
            })),
            failure_reset_seconds: 300,
        };

        let refs: Vec<&RpcEndpoint> = endpoints.iter().collect();

        // Run multiple selections to verify randomness
        let mut selected_urls = std::collections::HashSet::new();
        for _ in 0..100 {
            if let Some(ep) = pool.weighted_random_select(&refs) {
                selected_urls.insert(ep.url.clone());
            }
        }

        // Should select from all endpoints over many iterations
        assert!(
            selected_urls.len() > 1,
            "Should randomly select different endpoints"
        );
    }

    #[test]
    fn test_mark_failed() {
        let pool = EndpointPool::new();
        let url = pool.endpoints[0].url.clone();

        // Mark as failed
        pool.mark_failed(&url);

        // Check it's in failed list
        if let Ok(failed) = pool.failed_endpoints.lock() {
            assert!(failed.failures.contains_key(&url));
        }
    }

    #[test]
    fn test_available_endpoints_excludes_failed() {
        let endpoints = vec![
            RpcEndpoint {
                url: "http://test1".to_string(),
                weight: 50,
                max_retries: 3,
            },
            RpcEndpoint {
                url: "http://test2".to_string(),
                weight: 50,
                max_retries: 3,
            },
        ];

        let pool = EndpointPool {
            endpoints,
            failed_endpoints: Arc::new(Mutex::new(FailedEndpoints {
                failures: std::collections::HashMap::new(),
            })),
            failure_reset_seconds: 300,
        };

        // Initially all available
        assert_eq!(pool.available_endpoints().len(), 2);

        // Mark one as failed
        pool.mark_failed("http://test1");

        // Only one should be available
        assert_eq!(pool.available_endpoints().len(), 1);
        assert_eq!(pool.available_endpoints()[0].url, "http://test2");
    }
}
