use super::*;
use crate::pool_info::{PoolInfo, PoolInfoBared};
use near_sdk::json_types::U128;
use std::collections::HashSet;

fn make_test_pool(id: u32, fee: u32, amounts: Vec<u128>) -> Arc<PoolInfo> {
    let token_accounts: Vec<common::types::TokenAccount> = (0..amounts.len())
        .map(|i| format!("token_{i}.near").parse().unwrap())
        .collect();
    Arc::new(PoolInfo::new(
        id,
        PoolInfoBared {
            pool_kind: "SIMPLE_POOL".to_string(),
            token_account_ids: token_accounts,
            amounts: amounts.into_iter().map(U128).collect(),
            total_fee: fee,
            shares_total_supply: U128(0),
            amp: 0,
        },
        chrono::Utc::now().naive_utc(),
    ))
}

// --- TokenPairId ---

#[test]
fn test_token_pair_id_equality() {
    let id1 = TokenPairId {
        pool_id: 1,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    let id2 = TokenPairId {
        pool_id: 1,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    assert_eq!(id1, id2);
}

#[test]
fn test_token_pair_id_inequality() {
    let id1 = TokenPairId {
        pool_id: 1,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    let id2 = TokenPairId {
        pool_id: 2,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    assert_ne!(id1, id2);
}

#[test]
fn test_token_pair_id_hash() {
    let id1 = TokenPairId {
        pool_id: 1,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    let id2 = TokenPairId {
        pool_id: 1,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    let id3 = TokenPairId {
        pool_id: 2,
        token_in: TokenIn::from(0),
        token_out: TokenOut::from(1),
    };
    let mut set = HashSet::new();
    set.insert(id1);
    set.insert(id2);
    set.insert(id3);
    assert_eq!(set.len(), 2);
}

// --- TokenPairLike trait ---

#[test]
fn test_token_pair_like_pool_id() {
    let pool = make_test_pool(42, 30, vec![1000, 2000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    assert_eq!(pair.pool_id(), 42);
}

#[test]
fn test_token_pair_like_token_ids() {
    let pool = make_test_pool(1, 30, vec![1000, 2000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    assert_eq!(pair.token_in_id().to_string(), "token_0.near");
    assert_eq!(pair.token_out_id().to_string(), "token_1.near");
}

#[test]
fn test_token_pair_like_estimate_return() {
    let pool = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let result = pair.estimate_return(100).unwrap();
    assert!(result > 0);
    // fee=30/10000=0.3%, ratio=2:1 → 出力は ~199 だが手数料分やや少ない
    assert!(result < 200);
}

// --- TokenPair ---

#[test]
fn test_token_pair_pair_id() {
    let pool = make_test_pool(5, 30, vec![1000, 2000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let pair_id = pair.pair_id();
    assert_eq!(pair_id.pool_id, 5);
    assert_eq!(pair_id.token_in, TokenIn::from(0));
    assert_eq!(pair_id.token_out, TokenOut::from(1));
}

#[test]
fn test_token_pair_amount_in_out() {
    let pool = make_test_pool(1, 30, vec![1000, 2000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    assert_eq!(pair.amount_in().unwrap(), 1000);
    assert_eq!(pair.amount_out().unwrap(), 2000);
}

#[test]
fn test_token_pair_estimate_normal_return() {
    let pool = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let (in_value, out_value) = pair.estimate_normal_return().unwrap();
    assert_eq!(in_value, 500_000); // balance_in / 2
    assert!(out_value > 0);
}

#[test]
fn test_token_pair_estimate_normal_return_zero_balance() {
    let pool = make_test_pool(1, 30, vec![0, 2000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let result = pair.estimate_normal_return();
    assert!(result.is_err());
}

// --- TokenPath ---

#[test]
fn test_token_path_empty() {
    let path = TokenPath(vec![]);
    assert_eq!(path.len(), 0);
    assert!(path.is_empty());
}

#[test]
fn test_token_path_len() {
    let pool = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let path = TokenPath(vec![pair]);
    assert_eq!(path.len(), 1);
    assert!(!path.is_empty());
}

#[test]
fn test_token_path_calc_value_zero_initial() {
    let pool = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let path = TokenPath(vec![pair]);
    assert_eq!(path.calc_value(0).unwrap(), 0);
}

#[test]
fn test_token_path_calc_value_single_hop() {
    let pool = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pair = pool.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let path = TokenPath(vec![pair]);
    let result = path.calc_value(100).unwrap();
    assert!(result > 0);
}

#[test]
fn test_token_path_calc_value_multi_hop() {
    let pool1 = make_test_pool(1, 30, vec![1_000_000, 2_000_000]);
    let pool2 = make_test_pool(2, 30, vec![2_000_000, 3_000_000]);
    let pair1 = pool1.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let pair2 = pool2.get_pair(TokenIn::from(0), TokenOut::from(1)).unwrap();
    let path = TokenPath(vec![pair1, pair2]);
    let result = path.calc_value(1000).unwrap();
    assert!(result > 0);
}

// --- Error cases ---

#[test]
fn test_get_pair_same_token_error() {
    let pool = make_test_pool(1, 30, vec![1000, 2000]);
    let result = pool.get_pair(TokenIn::from(0), TokenOut::from(0));
    assert!(result.is_err());
}

#[test]
fn test_get_pair_out_of_index_error() {
    let pool = make_test_pool(1, 30, vec![1000, 2000]);
    let result = pool.get_pair(TokenIn::from(0), TokenOut::from(5));
    assert!(result.is_err());
}
