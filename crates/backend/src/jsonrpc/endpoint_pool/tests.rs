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

#[test]
fn test_rate_limit_triggers_endpoint_switch() {
    let endpoints = vec![
        RpcEndpoint {
            url: "http://endpoint1".to_string(),
            weight: 50,
            max_retries: 3,
        },
        RpcEndpoint {
            url: "http://endpoint2".to_string(),
            weight: 50,
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

    // Simulate rate limit on endpoint1
    pool.mark_failed("http://endpoint1");

    // Next endpoint selection should exclude endpoint1
    let selected = pool.next_endpoint();
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().url, "http://endpoint2");

    // Verify endpoint1 is in failed list
    if let Ok(failed) = pool.failed_endpoints.lock() {
        assert!(failed.failures.contains_key("http://endpoint1"));
        assert!(!failed.failures.contains_key("http://endpoint2"));
    }
}

#[test]
fn test_all_endpoints_failed_resets() {
    let endpoints = vec![
        RpcEndpoint {
            url: "http://endpoint1".to_string(),
            weight: 50,
            max_retries: 3,
        },
        RpcEndpoint {
            url: "http://endpoint2".to_string(),
            weight: 50,
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

    // Mark all endpoints as failed
    pool.mark_failed("http://endpoint1");
    pool.mark_failed("http://endpoint2");

    // Should reset all failures and return first endpoint
    let selected = pool.next_endpoint();
    assert!(selected.is_some());
    assert_eq!(selected.unwrap().url, "http://endpoint1");

    // Verify failures were cleared
    if let Ok(failed) = pool.failed_endpoints.lock() {
        assert!(failed.failures.is_empty());
    }
}
