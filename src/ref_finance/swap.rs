use crate::logging::*;
use crate::ref_finance::pool_info::{TokenPair, TokenPairLike};
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::wallet::Wallet;
use crate::{jsonrpc, Result};
use near_primitives::types::Balance;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Single swap action.
#[derive(Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    /// Pool which should be used for swapping.
    pub pool_id: u64,
    /// Token to swap from.
    pub token_in: AccountId,
    /// Amount to exchange.
    /// If amount_in is None, it will take amount_out from previous step.
    /// Will fail if amount_in is None on the first step.
    pub amount_in: Option<U128>,
    /// Token to swap into.
    pub token_out: AccountId,
    /// Required minimum amount of token_out.
    pub min_amount_out: U128,
}
const METHOD_NAME: &str = "swap";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OutputValue {
    pub estimated: Balance,
    pub minimum: Balance,
}

/// 単一のスワップアクションを生成する関数
pub fn create_swap_action<T>(
    pair: &T,
    prev_out: OutputValue,
    is_first_swap: bool,
    min_out_ratio: f32,
) -> Result<(SwapAction, OutputValue)>
where
    T: TokenPairLike,
{
    let amount_in = is_first_swap.then_some(U128(prev_out.estimated));
    let pool_id = pair.pool_id();
    let token_in = pair.token_in_id();
    let token_out = pair.token_out_id();
    let estimated = pair.estimate_return(prev_out.estimated)?;
    let minimum = pair.estimate_return(prev_out.minimum)?;
    let minimum = ((minimum as f32) * min_out_ratio).ceil() as Balance;

    let action = SwapAction {
        pool_id,
        token_in: token_in.as_id().to_owned(),
        amount_in,
        token_out: token_out.as_id().to_owned(),
        min_amount_out: U128(minimum),
    };

    Ok((action, OutputValue { estimated, minimum }))
}

/// パスに沿って複数のスワップアクションを生成する関数
pub fn build_swap_actions<T>(
    path: &[T],
    initial: Balance,
    min_out_ratio: f32,
) -> Result<(Vec<SwapAction>, OutputValue)>
where
    T: TokenPairLike,
{
    let log = DEFAULT.new(o!(
        "function" => "build_swap_actions",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
        "min_out_ratio" => min_out_ratio,
    ));
    info!(log, "building swap actions");

    let mut actions = Vec::new();
    let out = path.iter().try_fold(
        OutputValue {
            estimated: initial,
            minimum: initial,
        },
        |prev_out, pair| -> Result<OutputValue> {
            let is_first = prev_out.estimated == initial;
            let (action, next_out) = create_swap_action(pair, prev_out, is_first, min_out_ratio)?;

            // デバッグログをここで出力
            debug!(log, "created swap action";
                "pool_id" => action.pool_id,
                "token_in" => format!("{}", action.token_in),
                "amount_in" => action.amount_in.map_or(0, |u| u.0),
                "token_out" => format!("{}", action.token_out),
                "min_amount_out" => action.min_amount_out.0,
                "estimated_out" => next_out.estimated,
            );

            actions.push(action);
            Ok(next_out)
        },
    )?;

    info!(log, "finished building swap actions";
        o!("estimated_total_out" => format!("{:?}", out)),
    );
    Ok((actions, out))
}

pub async fn run_swap<A, W>(
    client: &A,
    wallet: &W,
    path: &[TokenPair],
    initial: Balance,
    under_limit: f32,
) -> Result<(A::Output, OutputValue)>
where
    A: jsonrpc::SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
        "under_limit" => under_limit,
    ));
    info!(log, "entered");

    let ratio_by_step = calculate_ratio_by_step(initial, under_limit, path.len());
    let (actions, out) = build_swap_actions(path, initial, ratio_by_step)?;

    let args = json!({
        "actions": actions,
    });

    let deposit = 1;
    let signer = wallet.signer();

    let tx_hash = client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, args, deposit)
        .await?;

    Ok((tx_hash, out))
}

pub fn gather_token_accounts(pairs_list: &[&[TokenPair]]) -> Vec<TokenAccount> {
    let mut tokens = Vec::new();
    for pairs in pairs_list.iter() {
        for pair in pairs.iter() {
            tokens.push(pair.token_in_id().into());
            tokens.push(pair.token_out_id().into());
        }
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

// output_valueとgainから、ratio_by_stepを計算する関数
fn calculate_ratio_by_step(output_value: Balance, under_limit: f32, steps: usize) -> f32 {
    let log = DEFAULT.new(o!(
        "function" => "calculate_ratio_by_step",
        "output_value" => format!("{}", output_value),
        "under_limit" => format!("{}", under_limit),
        "steps" => format!("{}", steps),
    ));

    let under_ratio = under_limit / (output_value as f32);

    // stepsの乗根を計算（steps乗してunder_ratioになるような値）
    let ratio_by_step = under_ratio.powf(1.0 / steps as f32);

    info!(log, "calculated";
        "under_limit" => ?under_limit,
        "ratio_by_step" => ?ratio_by_step,
    );
    ratio_by_step
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
    use proptest::prelude::*;
    use proptest::prop_oneof;
    use proptest::strategy::Just;

    struct MockTokenPair {
        pool_id: u64,
        token_in: TokenAccount,
        token_out: TokenAccount,
        rate: f32,
    }

    impl TokenPairLike for MockTokenPair {
        fn pool_id(&self) -> u64 {
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
    fn test_create_swap_action() {
        let pair = MockTokenPair {
            pool_id: 123,
            token_in: "token_a".parse().unwrap(),
            token_out: "token_b".parse().unwrap(),
            rate: 0.9,
        };

        // 最初のスワップアクション（amount_inが設定される）
        let prev_out = OutputValue {
            estimated: 1000,
            minimum: 1000,
        };
        let min_out_ratio = 0.8;

        let (action, output) = create_swap_action(&pair, prev_out, true, min_out_ratio).unwrap();

        // 検証
        assert_eq!(action.pool_id, 123);
        assert_eq!(action.token_in.to_string(), "token_a");
        assert_eq!(action.token_out.to_string(), "token_b");
        assert_eq!(action.amount_in, Some(U128(1000)));
        assert_eq!(action.min_amount_out.0, 720); // 1000 * 0.9 * 0.8 = 720

        // 期待される出力値
        assert_eq!(output.estimated, 900); // 1000 * 0.9 = 900
        assert_eq!(output.minimum, 720); // 1000 * 0.9 * 0.8 = 720

        // 連続したスワップアクション（amount_inはNone）
        let (action2, output2) = create_swap_action(&pair, output, false, min_out_ratio).unwrap();

        // 検証
        assert_eq!(action2.amount_in, None);
        assert_eq!(output2.estimated, 810); // 900 * 0.9 = 810
        assert_eq!(output2.minimum, 519); // 720 * 0.9 * 0.8 = 518.3999 -> 519 切り上げられる
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

        let initial = 1000;
        let min_out_ratio = 0.8;

        let (actions, output) = build_swap_actions(&[pair], initial, min_out_ratio).unwrap();

        // 検証
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].pool_id, 1);
        assert_eq!(actions[0].token_in.to_string(), "token_a");
        assert_eq!(actions[0].token_out.to_string(), "token_b");
        assert_eq!(actions[0].amount_in, Some(U128(1000)));
        assert_eq!(actions[0].min_amount_out.0, 720); // 1000 * 0.9 * 0.8

        assert_eq!(output.estimated, 900); // 1000 * 0.9
        assert_eq!(output.minimum, 720); // 1000 * 0.9 * 0.8
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

        let initial = 1000;
        let min_out_ratio = 0.8;

        let path = vec![pair1, pair2, pair3];
        let (actions, output) = build_swap_actions(&path, initial, min_out_ratio).unwrap();

        // 検証
        assert_eq!(actions.len(), 3);

        // 最初のアクションの検証
        assert_eq!(actions[0].pool_id, 1);
        assert_eq!(actions[0].token_in.to_string(), "token_a");
        assert_eq!(actions[0].token_out.to_string(), "token_b");
        assert_eq!(actions[0].amount_in, Some(U128(1000)));
        assert_eq!(actions[0].min_amount_out.0, 720); // 1000 * 0.9 * 0.8

        // 2番目のアクションの検証
        assert_eq!(actions[1].pool_id, 2);
        assert_eq!(actions[1].token_in.to_string(), "token_b");
        assert_eq!(actions[1].token_out.to_string(), "token_c");
        assert_eq!(actions[1].amount_in, None);
        assert_eq!(
            actions[1].min_amount_out.0,
            (actions[0].min_amount_out.0 as f32 * path[1].rate * min_out_ratio).ceil() as u128
        );

        // 3番目のアクションの検証
        assert_eq!(actions[2].pool_id, 3);
        assert_eq!(actions[2].token_in.to_string(), "token_c");
        assert_eq!(actions[2].token_out.to_string(), "token_d");
        assert_eq!(actions[2].amount_in, None);
        assert_eq!(
            actions[2].min_amount_out.0,
            (actions[1].min_amount_out.0 as f32 * path[2].rate * min_out_ratio).ceil() as u128
        );

        // 最終的な出力の検証
        // 1000 * 0.9 * 0.95 * 0.98 = 837.8999 ≈ 838
        let expected_estimate = (1000_f32 * path[0].rate * path[1].rate * path[2].rate) as u128;
        assert_eq!(output.estimated, expected_estimate);

        // 最小出力値の検証（各ステップでmin_out_ratioを適用）
        let expected_minimum = (((1000_f32 * path[0].rate * min_out_ratio)
            * (path[1].rate * min_out_ratio))
            * (path[2].rate * min_out_ratio))
            .ceil() as u128;
        assert_eq!(output.minimum, expected_minimum);
        assert_eq!(output.minimum, actions[2].min_amount_out.0);
    }

    #[test]
    fn test_build_swap_actions_empty_path() {
        // 空のパスでのテスト
        let initial = 1000;
        let min_out_ratio = 0.8;
        let path: Vec<MockTokenPair> = vec![];

        let result = build_swap_actions(&path, initial, min_out_ratio);

        // 期待される動作：空のアクションリストとinputと同じ値のoutputを返す
        match result {
            Ok((actions, output)) => {
                assert!(actions.is_empty());
                assert_eq!(output.estimated, initial);
                assert_eq!(output.minimum, initial);
            }
            Err(_) => panic!("Expected successful execution with empty path"),
        }
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

        let initial = 1; // 最小の金額
        let min_out_ratio = 0.8;

        let (actions, output) = build_swap_actions(&[pair], initial, min_out_ratio).unwrap();

        // 検証
        assert_eq!(actions.len(), 1);
        assert_eq!(output.estimated, 0); // 1 * 0.9 = 0.9 → 0 (整数の切り捨て)
        assert_eq!(actions[0].min_amount_out.0, 0);
    }

    proptest! {
        // calculate_ratio_by_stepのプロパティテスト
        #[test]
        fn prop_ratio_by_step_composes_correctly(
            output_value in 1_000_000_000_000_000_000_000_000..10_000_000_000_000_000_000_000_000u128,
            under_limit_ratio in 0.5f32..0.99f32,
            steps in prop_oneof![Just(1usize), 2usize..10usize]
        ) {
            let under_limit = (output_value as f32) * under_limit_ratio;

            let ratio_by_step = calculate_ratio_by_step(output_value, under_limit, steps);

            // ratio_by_stepをsteps回掛け合わせるとunder_ratioになることを確認
            // steps = 1 の場合は計算不要
            let total_ratio = if steps == 1 {
                ratio_by_step
            } else {
                ratio_by_step.powi(steps as i32)
            };
            let expected_ratio = under_limit / (output_value as f32);

            // 浮動小数点の精度の問題を考慮して、許容誤差を設定
            let epsilon = 0.001f32;

            // total_ratioがexpected_ratioに近いことを確認
            prop_assert!((total_ratio - expected_ratio).abs() <= epsilon);

            // 最終的な出力値がunder_limitに近いことを確認
            let final_output = (output_value as f32) * total_ratio;
            prop_assert!((final_output - under_limit).abs() <= epsilon * (output_value as f32));
        }
    }
}
