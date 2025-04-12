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
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct SwapAction {
    /// Pool which should be used for swapping.
    pub pool_id: u32,
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

#[derive(Debug, Clone)]
pub struct SwapArg {
    pub initial_in: Balance,
    pub min_out: Balance,
}

/// パスに沿って複数のスワップアクションを生成する関数
fn build_swap_actions<T>(path: &[T], arg: SwapArg) -> Result<(Vec<SwapAction>, Balance)>
where
    T: TokenPairLike,
{
    let log = DEFAULT.new(o!(
        "function" => "build_swap_actions",
        "path.length" => format!("{}", path.len()),
        "initial" => arg.initial_in,
        "min_out" => arg.min_out,
    ));
    info!(log, "building swap actions");
    if path.is_empty() {
        return Ok((Vec::new(), arg.initial_in));
    }

    let first_id = path[0].pool_id();
    let last_id = path[path.len() - 1].pool_id();

    let mut actions = Vec::new();
    let out = path
        .iter()
        .try_fold(arg.initial_in, |prev_out, pair| -> Result<Balance> {
            let is_first = pair.pool_id() == first_id;
            let min_out = if pair.pool_id() == last_id {
                arg.min_out
            } else {
                0
            };
            let next_out = pair.estimate_return(prev_out)?;

            let action = SwapAction {
                pool_id: pair.pool_id(),
                token_in: pair.token_in_id().as_id().to_owned(),
                amount_in: is_first.then_some(U128(prev_out)),
                token_out: pair.token_out_id().as_id().to_owned(),
                min_amount_out: U128(min_out),
            };

            debug!(log, "created swap action";
                "pool_id" => ?action,
                "estimated_out" => next_out,
            );

            actions.push(action);
            Ok(next_out)
        })?;

    info!(log, "finished building swap actions";
        o!("estimated_total_out" => format!("{:?}", out)),
    );
    Ok((actions, out))
}

pub async fn run_swap<A, W>(
    client: &A,
    wallet: &W,
    path: &[TokenPair],
    arg: SwapArg,
) -> Result<(A::Output, Balance)>
where
    A: jsonrpc::SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => arg.initial_in,
        "min_out" => arg.min_out,
    ));
    info!(log, "entered");

    let (actions, out) = build_swap_actions(path, arg)?;

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

#[cfg(test)]
mod tests {
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
}
