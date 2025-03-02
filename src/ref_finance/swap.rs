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

/// パスに沿って複数のスワップアクションを生成する関数
fn build_swap_actions<T>(path: &[T], initial: Balance) -> Result<(Vec<SwapAction>, Balance)>
where
    T: TokenPairLike,
{
    let log = DEFAULT.new(o!(
        "function" => "build_swap_actions",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
    ));
    info!(log, "building swap actions");
    if path.is_empty() {
        return Ok((Vec::new(), initial));
    }

    let first_id = path[0].pool_id();
    let last_id = path[path.len() - 1].pool_id();

    let mut actions = Vec::new();
    let out = path
        .iter()
        .try_fold(initial, |prev_out, pair| -> Result<Balance> {
            let is_first = pair.pool_id() == first_id;
            let min_out = if pair.pool_id() == last_id {
                initial + 1
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
                "pool_id" => action.pool_id,
                "token_in" => format!("{}", action.token_in),
                "amount_in" => action.amount_in.map_or(0, |u| u.0),
                "token_out" => format!("{}", action.token_out),
                "min_amount_out" => action.min_amount_out.0,
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
    initial: Balance,
) -> Result<(A::Output, Balance)>
where
    A: jsonrpc::SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "run_swap",
        "path.length" => format!("{}", path.len()),
        "initial" => initial,
    ));
    info!(log, "entered");

    let (actions, out) = build_swap_actions(path, initial)?;

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
    use near_sdk::require;

    use super::*;
    use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};

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
    fn test_build_swap_actions_single_pair() {
        // 単一のペアでのスワップテスト
        let pair = MockTokenPair {
            pool_id: 1,
            token_in: "token_a".parse().unwrap(),
            token_out: "token_b".parse().unwrap(),
            rate: 0.9,
        };

        let initial = 1000;

        let (actions, output) = build_swap_actions(&[pair], initial).unwrap();

        // 検証
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].pool_id, 1);
        assert_eq!(actions[0].token_in.to_string(), "token_a");
        assert_eq!(actions[0].token_out.to_string(), "token_b");
        assert_eq!(actions[0].amount_in, Some(U128(1000)));
        assert_eq!(actions[0].min_amount_out.0, 1001);

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

        let initial = 1000;

        let path = vec![pair1, pair2, pair3];
        let (actions, output) = build_swap_actions(&path, initial).unwrap();

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
        assert_eq!(actions[2].min_amount_out.0, 1001);

        // 最終的な出力の検証
        // 1000 * 0.9 * 0.95 * 0.98 = 837.8999 ≈ 838
        let expected_estimate = (1000_f32 * path[0].rate * path[1].rate * path[2].rate) as u128;
        assert_eq!(output, expected_estimate);
    }

    #[test]
    fn test_build_swap_actions_empty_path() {
        // 空のパスでのテスト
        let initial = 1000;
        let path: Vec<MockTokenPair> = vec![];

        let result = build_swap_actions(&path, initial);
        require!(result.is_ok());

        // 期待される動作：空のアクションリストとinputと同じ値のoutputを返す
        let (actions, output) = result.unwrap();
        assert!(actions.is_empty());
        assert_eq!(output, initial);
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

        let (actions, output) = build_swap_actions(&[pair], initial).unwrap();

        // 検証
        assert_eq!(actions.len(), 1);
        assert_eq!(output, 0); // 1 * 0.9 = 0.9 → 0 (整数の切り捨て)
        assert_eq!(actions[0].amount_in, Some(U128(1)));
        assert_eq!(actions[0].min_amount_out.0, 2);
    }
}
