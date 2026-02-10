use super::*;

// --- generate_combinations ---

#[test]
fn combinations_single_values() {
    let config = SweepConfig {
        top_tokens: vec![10],
        volatility_days: vec![7],
        price_history_days: vec![30],
        rebalance_threshold: vec![0.1],
        rebalance_interval_days: vec![1],
    };
    let combos = generate_combinations(&config);
    assert_eq!(combos.len(), 1);
    assert_eq!(combos[0].top_tokens, 10);
    assert_eq!(combos[0].volatility_days, 7);
    assert_eq!(combos[0].price_history_days, 30);
    assert!((combos[0].rebalance_threshold - 0.1).abs() < f64::EPSILON);
    assert_eq!(combos[0].rebalance_interval_days, 1);
}

#[test]
fn combinations_cartesian_product() {
    let config = SweepConfig {
        top_tokens: vec![5, 10],
        volatility_days: vec![7, 14],
        price_history_days: vec![30],
        rebalance_threshold: vec![0.05, 0.1, 0.2],
        rebalance_interval_days: vec![1],
    };
    let combos = generate_combinations(&config);
    // 2 * 2 * 1 * 3 * 1 = 12
    assert_eq!(combos.len(), 12);
}

#[test]
fn combinations_empty_dimension() {
    let config = SweepConfig {
        top_tokens: vec![],
        volatility_days: vec![7],
        price_history_days: vec![30],
        rebalance_threshold: vec![0.1],
        rebalance_interval_days: vec![1],
    };
    let combos = generate_combinations(&config);
    assert_eq!(combos.len(), 0);
}

#[test]
fn combinations_preserves_all_values() {
    let config = SweepConfig {
        top_tokens: vec![5, 10],
        volatility_days: vec![7],
        price_history_days: vec![30],
        rebalance_threshold: vec![0.1],
        rebalance_interval_days: vec![1],
    };
    let combos = generate_combinations(&config);
    assert_eq!(combos.len(), 2);
    assert_eq!(combos[0].top_tokens, 5);
    assert_eq!(combos[1].top_tokens, 10);
}

// --- SweepConfig deserialization ---

#[test]
fn sweep_config_defaults() {
    let json = "{}";
    let config: SweepConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.top_tokens, vec![10]);
    assert_eq!(config.volatility_days, vec![7]);
    assert_eq!(config.price_history_days, vec![30]);
    assert_eq!(config.rebalance_threshold, vec![0.1]);
    assert_eq!(config.rebalance_interval_days, vec![1]);
}

#[test]
fn sweep_config_custom_values() {
    let json = r#"{
        "top_tokens": [5, 10, 20],
        "volatility_days": [7, 14],
        "rebalance_threshold": [0.05, 0.1]
    }"#;
    let config: SweepConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.top_tokens, vec![5, 10, 20]);
    assert_eq!(config.volatility_days, vec![7, 14]);
    // defaults for unspecified fields
    assert_eq!(config.price_history_days, vec![30]);
    assert_eq!(config.rebalance_threshold, vec![0.05, 0.1]);
    assert_eq!(config.rebalance_interval_days, vec![1]);
}

#[test]
fn sweep_config_invalid_json() {
    let json = "not json";
    assert!(serde_json::from_str::<SweepConfig>(json).is_err());
}
