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

/// 単一のスワップアクションを生成する関数
pub fn create_swap_action<T>(
    pair: &T,
    prev_amount: Balance,
    is_first_swap: bool,
    min_out_ratio: f32,
) -> Result<(SwapAction, Balance)>
where
    T: TokenPairLike,
{
    let amount_in = is_first_swap.then_some(U128(prev_amount));
    let pool_id = pair.pool_id();
    let token_in = pair.token_in_id();
    let token_out = pair.token_out_id();
    let next_out = pair.estimate_return(prev_amount)?;
    let min_out = ((next_out as f32) * min_out_ratio) as Balance;

    let action = SwapAction {
        pool_id,
        token_in: token_in.as_id().to_owned(),
        amount_in,
        token_out: token_out.as_id().to_owned(),
        min_amount_out: U128(min_out),
    };

    Ok((action, next_out))
}

/// パスに沿って複数のスワップアクションを生成する関数
pub fn build_swap_actions<T>(
    path: &[T],
    initial: Balance,
    min_out_ratio: f32,
) -> Result<(Vec<SwapAction>, Balance)>
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
    let out = path
        .iter()
        .try_fold(initial, |prev_out, pair| -> Result<Balance> {
            let is_first = prev_out == initial;
            let (action, next_out) = create_swap_action(pair, prev_out, is_first, min_out_ratio)?;

            // デバッグログをここで出力
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

    info!(log, "finished building swap actions"; o!("estimated_total_out" => format!("{}", out)));
    Ok((actions, out))
}

pub async fn run_swap<A, W>(
    client: &A,
    wallet: &W,
    path: &[TokenPair],
    initial: Balance,
    under_limit: f32,
) -> Result<(A::Output, Balance)>
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
    use proptest::prelude::*;
    use proptest::prop_oneof;
    use proptest::strategy::Just;

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
