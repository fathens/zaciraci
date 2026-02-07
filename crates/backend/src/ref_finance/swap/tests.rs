use super::*;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use near_sdk::require;

struct MockTokenPair {
    pool_id: u32,
    token_in: TokenAccount,
    token_out: TokenAccount,
    rate: f32,
}

impl TokenPairLike for MockTokenPair {
    fn pool_id(&self) -> u32 {
        self.pool_id
    }

    fn token_in_id(&self) -> TokenInAccount {
        self.token_in.clone().into()
    }

    fn token_out_id(&self) -> TokenOutAccount {
        self.token_out.clone().into()
    }

    fn estimate_return(&self, amount_in: u128) -> Result<u128> {
        let amount_out = (amount_in as f32 * self.rate) as u128;
        Ok(amount_out)
    }
}

#[test]
fn test_build_swap_actions_single_pair() {
    // 単一のペアでのスワップテスト
    let pair = MockTokenPair {
        pool_id: 1,
        token_in: "token_a".parse().unwrap(),
        token_out: "token_b".parse().unwrap(),
        rate: 0.9,
    };
    let arg = SwapArg {
        initial_in: 1000,
        min_out: 1234,
    };

    let (actions, output) = build_swap_actions(&[pair], arg).unwrap();

    // 検証
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].pool_id, 1);
    assert_eq!(actions[0].token_in.to_string(), "token_a");
    assert_eq!(actions[0].token_out.to_string(), "token_b");
    assert_eq!(actions[0].amount_in, Some(U128(1000)));
    assert_eq!(actions[0].min_amount_out.0, 1234);

    assert_eq!(output, 900); // 1000 * 0.9
}

#[test]
fn test_build_swap_actions_multiple_pairs() {
    // 複数のペアを経由するスワップテスト
    let pair1 = MockTokenPair {
        pool_id: 1,
        token_in: "token_a".parse().unwrap(),
        token_out: "token_b".parse().unwrap(),
        rate: 0.9,
    };

    let pair2 = MockTokenPair {
        pool_id: 2,
        token_in: "token_b".parse().unwrap(),
        token_out: "token_c".parse().unwrap(),
        rate: 0.95,
    };

    let pair3 = MockTokenPair {
        pool_id: 3,
        token_in: "token_c".parse().unwrap(),
        token_out: "token_d".parse().unwrap(),
        rate: 0.98,
    };

    let path = vec![pair1, pair2, pair3];
    let arg = SwapArg {
        initial_in: 1000,
        min_out: 1234,
    };

    let (actions, output) = build_swap_actions(&path, arg).unwrap();
    // 検証
    assert_eq!(actions.len(), 3);

    // 最初のアクションの検証
    assert_eq!(actions[0].pool_id, 1);
    assert_eq!(actions[0].token_in.to_string(), "token_a");
    assert_eq!(actions[0].token_out.to_string(), "token_b");
    assert_eq!(actions[0].amount_in, Some(U128(1000)));
    assert_eq!(actions[0].min_amount_out.0, 0);

    // 2番目のアクションの検証
    assert_eq!(actions[1].pool_id, 2);
    assert_eq!(actions[1].token_in.to_string(), "token_b");
    assert_eq!(actions[1].token_out.to_string(), "token_c");
    assert_eq!(actions[1].amount_in, None);
    assert_eq!(actions[1].min_amount_out.0, 0);

    // 3番目のアクションの検証
    assert_eq!(actions[2].pool_id, 3);
    assert_eq!(actions[2].token_in.to_string(), "token_c");
    assert_eq!(actions[2].token_out.to_string(), "token_d");
    assert_eq!(actions[2].amount_in, None);
    assert_eq!(actions[2].min_amount_out.0, 1234);

    // 最終的な出力の検証
    // 1000 * 0.9 * 0.95 * 0.98 = 837.8999 ≈ 838
    let expected_estimate = (1000_f32 * path[0].rate * path[1].rate * path[2].rate) as u128;
    assert_eq!(output, expected_estimate);
}

#[test]
fn test_build_swap_actions_empty_path() {
    // 空のパスでのテスト
    let path: Vec<MockTokenPair> = vec![];

    let result = build_swap_actions(
        &path,
        SwapArg {
            initial_in: 1000,
            min_out: 1234,
        },
    );
    require!(result.is_ok());

    // 期待される動作：空のアクションリストとinputと同じ値のoutputを返す
    let (actions, output) = result.unwrap();
    assert!(actions.is_empty());
    assert_eq!(output, 1000);
}

#[test]
fn test_build_swap_actions_with_very_small_amounts() {
    // 非常に小さい金額のテスト
    let pair = MockTokenPair {
        pool_id: 1,
        token_in: "token_a".parse().unwrap(),
        token_out: "token_b".parse().unwrap(),
        rate: 0.9,
    };
    let arg = SwapArg {
        initial_in: 1, // 最小の金額
        min_out: 5,
    };

    let (actions, output) = build_swap_actions(&[pair], arg).unwrap();

    // 検証
    assert_eq!(actions.len(), 1);
    assert_eq!(output, 0); // 1 * 0.9 = 0.9 → 0 (整数の切り捨て)
    assert_eq!(actions[0].amount_in, Some(U128(1)));
    assert_eq!(actions[0].min_amount_out.0, 5);
}
