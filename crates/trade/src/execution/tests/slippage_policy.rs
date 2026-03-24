use super::*;

fn token_out(s: &str) -> TokenOutAccount {
    s.parse().unwrap()
}

#[test]
fn test_buy_policy_with_matching_token() {
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(token_out("token.near"), 0.05);

    let policy = buy_policy(&token_out("token.near"), &expected_returns);
    match policy {
        SlippagePolicy::FromExpectedReturn(er) => {
            assert!((er.as_ratio() - 0.05).abs() < f64::EPSILON);
        }
        SlippagePolicy::Unprotected => panic!("Expected FromExpectedReturn"),
    }
}

#[test]
fn test_buy_policy_with_missing_token() {
    let expected_returns = BTreeMap::new();

    let policy = buy_policy(&token_out("unknown.near"), &expected_returns);
    assert!(
        matches!(policy, SlippagePolicy::Unprotected),
        "Missing token should fall back to Unprotected"
    );
}

#[test]
fn test_buy_policy_with_negative_return() {
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(token_out("token.near"), -0.03);

    let policy = buy_policy(&token_out("token.near"), &expected_returns);
    match policy {
        SlippagePolicy::FromExpectedReturn(er) => {
            assert!((er.as_ratio() - (-0.03)).abs() < f64::EPSILON);
        }
        SlippagePolicy::Unprotected => panic!("Expected FromExpectedReturn"),
    }
}

#[test]
fn test_buy_policy_with_zero_return() {
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(token_out("token.near"), 0.0);

    let policy = buy_policy(&token_out("token.near"), &expected_returns);
    match policy {
        SlippagePolicy::FromExpectedReturn(er) => {
            assert!((er.as_ratio()).abs() < f64::EPSILON);
        }
        SlippagePolicy::Unprotected => panic!("Expected FromExpectedReturn"),
    }
}

#[test]
fn test_buy_policy_selects_correct_token() {
    let mut expected_returns = BTreeMap::new();
    expected_returns.insert(token_out("token_a.near"), 0.05);
    expected_returns.insert(token_out("token_b.near"), 0.10);

    let policy_a = buy_policy(&token_out("token_a.near"), &expected_returns);
    let policy_b = buy_policy(&token_out("token_b.near"), &expected_returns);

    match policy_a {
        SlippagePolicy::FromExpectedReturn(er) => {
            assert!((er.as_ratio() - 0.05).abs() < f64::EPSILON);
        }
        SlippagePolicy::Unprotected => panic!("Expected FromExpectedReturn for token_a"),
    }

    match policy_b {
        SlippagePolicy::FromExpectedReturn(er) => {
            assert!((er.as_ratio() - 0.10).abs() < f64::EPSILON);
        }
        SlippagePolicy::Unprotected => panic!("Expected FromExpectedReturn for token_b"),
    }
}
