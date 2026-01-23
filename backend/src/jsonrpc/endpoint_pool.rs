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
#[derive(Debug)]
pub struct EndpointPool {
    endpoints: Vec<RpcEndpoint>,
    failed_endpoints: Arc<Mutex<FailedEndpoints>>,
    failure_reset_seconds: u64,
}

#[derive(Debug)]
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
        use crate::logging::*;
        let log = DEFAULT.new(o!(
            "function" => "EndpointPool::next_endpoint",
        ));

        let available = self.available_endpoints();
        if available.is_empty() {
            warn!(log, "no available endpoints, resetting all failures");
            // Reset all failures if no endpoints available
            if let Ok(mut failed) = self.failed_endpoints.lock() {
                failed.failures.clear();
            }
            // Try again with first endpoint
            return self.endpoints.first();
        }

        let selected = self.weighted_random_select(&available);
        if let Some(ep) = selected {
            info!(log, "endpoint selected";
                "url" => &ep.url,
                "weight" => ep.weight,
                "available_count" => available.len()
            );
        }
        selected
    }

    /// Mark an endpoint as failed
    pub fn mark_failed(&self, url: &str) {
        use crate::logging::*;
        let log = DEFAULT.new(o!(
            "function" => "EndpointPool::mark_failed",
            "url" => url.to_string(),
            "failure_reset_seconds" => self.failure_reset_seconds,
        ));

        if let Ok(mut failed) = self.failed_endpoints.lock() {
            failed.failures.insert(
                url.to_string(),
                SystemTime::now() + Duration::from_secs(self.failure_reset_seconds),
            );
            warn!(log, "endpoint marked as failed";
                "reset_after_seconds" => self.failure_reset_seconds,
                "total_failed" => failed.failures.len()
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

#[cfg(test)]
mod tests;
