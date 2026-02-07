use super::*;
use assertables::*;
use proptest::prelude::*;

#[test]
fn test_calc_retry_duration() {
    let upper = Duration::from_secs(60);
    let limit = 128;
    let retry_dur = calc_retry_duration(upper, limit, 0.0);

    assert_eq!(retry_dur(0), Duration::ZERO);
    assert_eq!(retry_dur(1), Duration::ZERO);
    assert_eq!(retry_dur(limit), upper);
    assert_eq!(retry_dur(limit + 1), Duration::ZERO);
}

proptest! {
    #[test]
    fn test_calc_retry_duration_range(retry_count in 2u16..128) {
        let limit = 128u16;
        let upper = Duration::from_secs(128);
        let retry_dur = calc_retry_duration(upper, limit, 0.0);

        assert_gt!(retry_dur(retry_count), Duration::from_secs(retry_count as u64));
    }

    #[test]
    fn test_fluctuate_zero_y(fr in 0.0..1_f32) {
        let v = fluctuate(0.0, fr);
        assert_eq!(v, 0.0);
    }

    #[test]
    fn test_fluctuate_zero_fr(y in 0.0..1000_f32) {
        let v = fluctuate(y, 0.0);
        assert_eq!(v, y);
    }

    #[test]
    fn test_fluctuate(y in 1.0..1000_f32, fr in 0.01..1_f32) {
        let v = fluctuate(y, fr);
        assert_ge!(v, y - y * fr);
        assert_le!(v, y + y * fr);
    }
}

// ===== Retry Logic Mock Tests =====

use std::cell::RefCell;
use std::rc::Rc;

/// Simulated response for retry logic testing
#[derive(Clone, Debug)]
enum MockResponse {
    Success(String),
    Retry,
    SwitchEndpoint,
}

/// Tracks retry logic behavior
#[derive(Debug, Default)]
struct RetryTracker {
    endpoint_calls: Vec<(String, u32)>, // (endpoint_url, endpoint_retry_count)
    total_retries: u16,
    endpoints_switched: Vec<String>,
    final_result: Option<std::result::Result<String, String>>,
}

/// Simulates the retry logic without actual network calls
fn simulate_retry_logic(
    endpoints: Vec<(String, u32)>, // (url, max_retries)
    retry_limit: u16,
    mut response_generator: impl FnMut(&str, u32) -> MockResponse,
) -> RetryTracker {
    let mut tracker = RetryTracker::default();
    let mut failed_endpoints: std::collections::HashSet<String> = std::collections::HashSet::new();

    'outer: loop {
        // Select first available endpoint (simulates weighted random selection)
        let available: Vec<_> = endpoints
            .iter()
            .filter(|(url, _)| !failed_endpoints.contains(url))
            .collect();

        let (endpoint_url, max_retries) = if available.is_empty() {
            // Reset failures and use first endpoint
            failed_endpoints.clear();
            endpoints.first().unwrap().clone()
        } else {
            // Select first available endpoint
            (*available.first().unwrap()).clone()
        };

        let mut endpoint_retry_count: u32 = 0;

        // Inner loop: retry on same endpoint
        loop {
            tracker
                .endpoint_calls
                .push((endpoint_url.clone(), endpoint_retry_count));

            match response_generator(&endpoint_url, endpoint_retry_count) {
                MockResponse::Success(result) => {
                    tracker.final_result = Some(Ok(result));
                    break 'outer;
                }
                MockResponse::Retry => {
                    tracker.total_retries += 1;
                    if tracker.total_retries > retry_limit {
                        tracker.final_result = Some(Err("global retry limit reached".to_string()));
                        break 'outer;
                    }

                    endpoint_retry_count += 1;
                    if endpoint_retry_count > max_retries {
                        // Switch to next endpoint
                        tracker.endpoints_switched.push(endpoint_url.clone());
                        failed_endpoints.insert(endpoint_url.clone());
                        break; // Break inner loop
                    }
                    // Continue retrying on same endpoint
                }
                MockResponse::SwitchEndpoint => {
                    tracker.total_retries += 1;
                    if tracker.total_retries > retry_limit {
                        tracker.final_result = Some(Err("global retry limit reached".to_string()));
                        break 'outer;
                    }

                    // Switch to next endpoint immediately
                    tracker.endpoints_switched.push(endpoint_url.clone());
                    failed_endpoints.insert(endpoint_url.clone());
                    break; // Break inner loop
                }
            }
        }
    }

    tracker
}

#[test]
fn test_retry_on_same_endpoint_until_max_retries() {
    // Test: retry on same endpoint max_retries times before switching
    let endpoints = vec![
        ("http://ep1".to_string(), 3), // max_retries = 3
        ("http://ep2".to_string(), 3),
    ];

    let call_count = Rc::new(RefCell::new(0));
    let call_count_clone = call_count.clone();

    let tracker = simulate_retry_logic(endpoints, 10, move |url, _| {
        let count = *call_count_clone.borrow();
        *call_count_clone.borrow_mut() = count + 1;

        // Fail first 4 calls (initial + 3 retries), then succeed
        if count < 4 && url == "http://ep1" {
            MockResponse::Retry
        } else {
            MockResponse::Success("ok".to_string())
        }
    });

    // Should have called ep1 4 times (initial + 3 retries)
    let ep1_calls: Vec<_> = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep1")
        .collect();
    assert_eq!(ep1_calls.len(), 4);

    // Should have switched to ep2 after max_retries exceeded
    assert_eq!(tracker.endpoints_switched, vec!["http://ep1"]);

    // Final call should be on ep2
    assert_eq!(
        tracker.endpoint_calls.last().unwrap().0,
        "http://ep2".to_string()
    );
    assert!(tracker.final_result.unwrap().is_ok());
}

#[test]
fn test_max_retries_zero_switches_immediately() {
    // Test: max_retries = 0 means switch immediately on first failure
    let endpoints = vec![
        ("http://ep1".to_string(), 0), // max_retries = 0
        ("http://ep2".to_string(), 0),
    ];

    let tracker = simulate_retry_logic(endpoints, 10, |url, retry_count| {
        if url == "http://ep1" && retry_count == 0 {
            MockResponse::Retry
        } else {
            MockResponse::Success("ok".to_string())
        }
    });

    // Should have called ep1 only once, then switched
    let ep1_calls: Vec<_> = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep1")
        .collect();
    assert_eq!(ep1_calls.len(), 1);

    // Should have switched to ep2
    assert_eq!(tracker.endpoints_switched, vec!["http://ep1"]);
    assert!(tracker.final_result.unwrap().is_ok());
}

#[test]
fn test_too_many_requests_switches_immediately() {
    // Test: SwitchEndpoint (TooManyRequests) switches immediately without retrying
    let endpoints = vec![
        ("http://ep1".to_string(), 3), // max_retries = 3
        ("http://ep2".to_string(), 3),
    ];

    let tracker = simulate_retry_logic(endpoints, 10, |url, _| {
        if url == "http://ep1" {
            MockResponse::SwitchEndpoint // Simulates TooManyRequests
        } else {
            MockResponse::Success("ok".to_string())
        }
    });

    // Should have called ep1 only once, then switched (no endpoint retries)
    let ep1_calls: Vec<_> = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep1")
        .collect();
    assert_eq!(ep1_calls.len(), 1);

    // Should have switched to ep2 immediately
    assert_eq!(tracker.endpoints_switched, vec!["http://ep1"]);
    assert!(tracker.final_result.unwrap().is_ok());
}

#[test]
fn test_global_retry_limit_stops_all_retries() {
    // Test: global retry_limit stops retries across all endpoints
    let endpoints = vec![
        ("http://ep1".to_string(), 3),
        ("http://ep2".to_string(), 3),
        ("http://ep3".to_string(), 3),
    ];

    let tracker = simulate_retry_logic(endpoints, 5, |_, _| {
        MockResponse::Retry // Always fail
    });

    // Total retries should be limited to 5
    assert_eq!(tracker.total_retries, 6); // 5 retries + 1 that exceeded limit

    // Should have an error result
    assert!(tracker.final_result.unwrap().is_err());
}

#[test]
fn test_endpoint_retry_count_resets_on_switch() {
    // Test: endpoint_retry_count resets when switching endpoints
    let endpoints = vec![("http://ep1".to_string(), 2), ("http://ep2".to_string(), 2)];

    let call_count = Rc::new(RefCell::new(0));
    let call_count_clone = call_count.clone();

    let tracker = simulate_retry_logic(endpoints, 10, move |_, _| {
        let count = *call_count_clone.borrow();
        *call_count_clone.borrow_mut() = count + 1;

        // Fail 6 times, then succeed
        if count < 6 {
            MockResponse::Retry
        } else {
            MockResponse::Success("ok".to_string())
        }
    });

    // ep1: initial(0) + 2 retries = 3 calls, then switch
    // ep2: initial(0) + 2 retries = 3 calls, then switch
    // ep1 again (failures reset): succeed on first try

    // Verify endpoint_retry_count values
    let ep1_retry_counts: Vec<_> = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep1")
        .map(|(_, count)| *count)
        .collect();
    let ep2_retry_counts: Vec<_> = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep2")
        .map(|(_, count)| *count)
        .collect();

    // First batch of ep1 calls: 0, 1, 2
    assert_eq!(&ep1_retry_counts[..3], &[0, 1, 2]);
    // ep2 calls: 0, 1, 2
    assert_eq!(ep2_retry_counts, vec![0, 1, 2]);
    // Second batch of ep1 calls should start at 0 again
    assert_eq!(ep1_retry_counts[3], 0);

    assert!(tracker.final_result.unwrap().is_ok());
}

#[test]
fn test_all_endpoints_failed_resets_and_retries() {
    // Test: when all endpoints fail, failures reset and retry from first
    let endpoints = vec![("http://ep1".to_string(), 1), ("http://ep2".to_string(), 1)];

    let call_count = Rc::new(RefCell::new(0));
    let call_count_clone = call_count.clone();

    let tracker = simulate_retry_logic(endpoints, 10, move |_, _| {
        let count = *call_count_clone.borrow();
        *call_count_clone.borrow_mut() = count + 1;

        // Fail 4 times (2 endpoints * 2 calls each), then succeed
        if count < 4 {
            MockResponse::Retry
        } else {
            MockResponse::Success("ok".to_string())
        }
    });

    // Both endpoints should have been switched
    assert!(
        tracker
            .endpoints_switched
            .contains(&"http://ep1".to_string())
    );
    assert!(
        tracker
            .endpoints_switched
            .contains(&"http://ep2".to_string())
    );

    // After reset, should have succeeded on ep1
    assert!(tracker.final_result.unwrap().is_ok());
}

#[test]
fn test_mixed_retry_and_switch_endpoint_responses() {
    // Test: combination of Retry and SwitchEndpoint responses
    let endpoints = vec![
        ("http://ep1".to_string(), 3),
        ("http://ep2".to_string(), 3),
        ("http://ep3".to_string(), 3),
    ];

    let ep2_retry_count = Rc::new(RefCell::new(0));
    let ep2_retry_count_clone = ep2_retry_count.clone();

    let tracker = simulate_retry_logic(endpoints, 15, move |url, _| match url {
        "http://ep1" => MockResponse::SwitchEndpoint, // Rate limited, switch immediately
        "http://ep2" => {
            let count = *ep2_retry_count_clone.borrow();
            *ep2_retry_count_clone.borrow_mut() = count + 1;
            if count < 4 {
                MockResponse::Retry // Retry 3 times (0,1,2,3), then switch
            } else {
                MockResponse::Success("ok".to_string())
            }
        }
        "http://ep3" => MockResponse::Success("ok".to_string()),
        _ => MockResponse::Success("ok".to_string()),
    });

    // ep1 should switch immediately (SwitchEndpoint)
    // ep2 should retry until max_retries exceeded, then switch
    // ep3 should succeed

    let ep1_calls = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep1")
        .count();
    let ep2_calls = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep2")
        .count();
    let ep3_calls = tracker
        .endpoint_calls
        .iter()
        .filter(|(url, _)| url == "http://ep3")
        .count();

    assert_eq!(ep1_calls, 1); // Immediate switch due to SwitchEndpoint
    assert_eq!(ep2_calls, 4); // 1 initial + 3 retries (max_retries=3)
    assert_eq!(ep3_calls, 1); // Success on first try

    assert!(tracker.final_result.unwrap().is_ok());
}
