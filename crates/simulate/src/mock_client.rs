use crate::portfolio_state::{
    self, DEFAULT_DECIMALS, PortfolioState, SwapEvent, SwapMethod, SwapResult, to_u128_or_warn,
};
use bigdecimal::BigDecimal;
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx, ViewContract};
use blockchain::ref_finance::swap::SwapAction;
use blockchain::types::gas_price::GasPrice;
use chrono::{DateTime, Utc};
use common::types::{TokenAccount, TokenAmount};
use logging::*;
use near_crypto::InMemorySigner;
use near_primitives::action::Action;
use near_primitives::types::BlockId;
use near_primitives::views::{
    CallResult, FinalExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
};
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simulated NEAR blockchain client for backtesting.
///
/// **Lock ordering**: When acquiring multiple locks, always lock `sim_day`
/// before `portfolio` to avoid deadlocks. This order must be consistent
/// across all call sites.
pub struct SimulationClient {
    portfolio: Arc<Mutex<PortfolioState>>,
    initial_native: u128,
    sim_day: Arc<Mutex<DateTime<Utc>>>,
}

fn decimals_for(token: &TokenAccount) -> u8 {
    trade::token_cache::get_cached_decimals(token).unwrap_or(DEFAULT_DECIMALS)
}

impl SimulationClient {
    pub fn new(
        portfolio: Arc<Mutex<PortfolioState>>,
        initial_native: u128,
        sim_day: Arc<Mutex<DateTime<Utc>>>,
    ) -> Self {
        Self {
            portfolio,
            initial_native,
            sim_day,
        }
    }
}

/// Estimate swap output by walking SwapAction hops through pool `estimate_return`.
///
/// This is a pure calculation (no I/O). Each hop's output feeds into the next
/// hop's input, so `SwapAction::amount_in` fields are intentionally ignored —
/// only the `amount_in` argument is used as the initial input for the first hop.
///
/// Returns `None` if any pool is missing from the list or a token is not found
/// in the pool's token list.
fn estimate_swap_via_pools(
    pools: &dex::PoolInfoList,
    swap_actions: &[SwapAction],
    amount_in: u128,
) -> Option<u128> {
    debug_assert!(
        swap_actions
            .windows(2)
            .all(|w| w[0].token_out == w[1].token_in),
        "swap action chain is not connected"
    );
    let mut current_amount = amount_in;
    for action in swap_actions {
        let pool = pools.get(action.pool_id).ok()?;
        let in_idx = pool
            .tokens()
            .position(|t| t.as_account_id() == &action.token_in)?;
        let out_idx = pool
            .tokens()
            .position(|t| t.as_account_id() == &action.token_out)?;
        current_amount = pool
            .estimate_return(
                dex::TokenIn::from(in_idx),
                current_amount,
                dex::TokenOut::from(out_idx),
            )
            .ok()?;
    }
    Some(current_amount)
}

impl SimulationClient {
    /// Calculate swap output by walking SwapAction hops through pool estimate_return.
    /// Falls back to DB rate conversion if pool data is unavailable.
    async fn calculate_swap_output_via_pools(
        &self,
        swap_actions: &[SwapAction],
        amount_in: u128,
        sim_day: DateTime<Utc>,
    ) -> Option<u128> {
        let log = DEFAULT.new(o!("function" => "calculate_swap_output_via_pools"));
        let pools = persistence::pool_info::read_from_db(Some(sim_day.naive_utc()))
            .await
            .inspect_err(|e| warn!(log, "failed to read pool data from DB"; "error" => %e))
            .ok()?;
        estimate_swap_via_pools(&pools, swap_actions, amount_in)
    }

    /// Fallback: calculate swap output using DB rates (no fee/slippage).
    async fn calculate_swap_output_via_rates(
        &self,
        token_in: &TokenAccount,
        amount_in: u128,
        token_out: &TokenAccount,
        sim_day: DateTime<Utc>,
    ) -> u128 {
        use common::types::{TokenAmount, YoctoValue};
        let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
        let wnear_in = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

        // token_in -> NEAR value
        let near_value = if token_in == wnear {
            YoctoValue::from_yocto(BigDecimal::from(amount_in)).to_near()
        } else {
            let token_in_out = token_in.to_out();

            let rate =
                match portfolio_state::get_rate_at_date(&token_in_out, &wnear_in, sim_day).await {
                    Some(r) => r,
                    None => return 0,
                };

            let decimals_in = decimals_for(token_in);
            let token_amount =
                TokenAmount::from_smallest_units(BigDecimal::from(amount_in), decimals_in);
            &token_amount / &rate
        };

        // NEAR value -> token_out amount
        if token_out == wnear {
            to_u128_or_warn(
                near_value.to_yocto().as_bigdecimal(),
                "swap_rate_near_to_yocto",
            )
        } else {
            let token_out_out = token_out.to_out();

            let rate =
                match portfolio_state::get_rate_at_date(&token_out_out, &wnear_in, sim_day).await {
                    Some(r) => r,
                    None => return 0,
                };

            let token_amount = &near_value * &rate;
            to_u128_or_warn(token_amount.smallest_units(), "swap_rate_token_amount")
        }
    }

    async fn handle_swap(&self, args_value: serde_json::Value) -> anyhow::Result<u128> {
        let log = DEFAULT.new(o!("function" => "SimulationClient::handle_swap"));

        // Acquire sim_day once upfront. All sub-methods receive it by value,
        // ensuring a consistent timestamp throughout the swap and avoiding
        // repeated lock acquisitions.
        let sim_day = *self.sim_day.lock().await;

        let Some(actions_array) = args_value.get("actions") else {
            return Ok(0);
        };
        let swap_actions: Vec<SwapAction> = match serde_json::from_value(actions_array.clone()) {
            Ok(actions) => actions,
            Err(e) => {
                warn!(log, "failed to parse swap actions, skipping"; "error" => %e);
                return Ok(0);
            }
        };
        if swap_actions.is_empty() {
            return Ok(0);
        }

        let first = &swap_actions[0];
        let last = swap_actions.last().expect("checked non-empty above");
        let token_in_account = TokenAccount::from(first.token_in.clone());
        let token_out_account = TokenAccount::from(last.token_out.clone());
        let amount_in = first.amount_in.map(u128::from).unwrap_or(0);
        if amount_in == 0 {
            return Ok(0);
        }

        // Try pool-based estimate_return first (fee + slippage aware)
        let (amount_out, swap_method) = match self
            .calculate_swap_output_via_pools(&swap_actions, amount_in, sim_day)
            .await
        {
            Some(out) => (out, SwapMethod::PoolBased),
            None => {
                // Fallback to DB rate conversion (no fee/slippage)
                warn!(log, "pool data unavailable, falling back to DB rate";
                    "token_in" => %token_in_account, "token_out" => %token_out_account
                );
                let out = self
                    .calculate_swap_output_via_rates(
                        &token_in_account,
                        amount_in,
                        &token_out_account,
                        sim_day,
                    )
                    .await;
                (out, SwapMethod::DbRate)
            }
        };

        if amount_out == 0 {
            warn!(log, "swap output is zero, skipping";
                "token_in" => %token_in_account, "token_out" => %token_out_account
            );
            return Ok(0);
        }

        // sim_day was acquired at the top of handle_swap. Only portfolio
        // needs locking here.
        let mut state = self.portfolio.lock().await;
        let actual = state.execute_simulated_swap(
            &token_in_account,
            amount_in,
            &token_out_account,
            amount_out,
        );

        let Some(SwapResult {
            actual_in,
            actual_out,
        }) = actual
        else {
            trace!(log, "swap skipped (insufficient balance or zero output)";
                "token_in" => %token_in_account, "token_out" => %token_out_account
            );
            return Ok(0);
        };

        let decimals_in = decimals_for(&token_in_account);
        let decimals_out = decimals_for(&token_out_account);
        state.swap_events.push(SwapEvent {
            timestamp: sim_day,
            token_in: token_in_account.clone(),
            amount_in: TokenAmount::from_smallest_units(BigDecimal::from(actual_in), decimals_in),
            token_out: token_out_account.clone(),
            amount_out: TokenAmount::from_smallest_units(
                BigDecimal::from(actual_out),
                decimals_out,
            ),
            swap_method,
            pool_ids: swap_actions.iter().map(|a| a.pool_id).collect(),
        });

        trace!(log, "simulated swap";
            "token_in" => %token_in_account,
            "amount_in" => actual_in,
            "token_out" => %token_out_account,
            "amount_out" => actual_out,
            "swap_method" => ?swap_method
        );

        Ok(actual_out)
    }
}

impl AccountInfo for SimulationClient {
    async fn get_native_amount(&self, _account: &AccountId) -> anyhow::Result<NearToken> {
        Ok(NearToken::from_yoctonear(self.initial_native))
    }
}

impl GasInfo for SimulationClient {
    async fn get_gas_price(&self, _block: Option<BlockId>) -> anyhow::Result<GasPrice> {
        Ok(GasPrice::from_balance(NearToken::from_yoctonear(
            100_000_000,
        )))
    }
}

impl SendTx for SimulationClient {
    type Output = MockSentTx;

    async fn transfer_native_token(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _amount: NearToken,
    ) -> anyhow::Result<Self::Output> {
        Ok(MockSentTx { output_amount: 0 })
    }

    async fn exec_contract<T>(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        method_name: &str,
        args: T,
        _deposit: NearToken,
    ) -> anyhow::Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        if method_name == "swap" {
            let args_value = serde_json::to_value(&args)?;
            let output_amount = self.handle_swap(args_value).await?;
            return Ok(MockSentTx { output_amount });
        }

        Ok(MockSentTx { output_amount: 0 })
    }

    async fn send_tx(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<Action>,
    ) -> anyhow::Result<Self::Output> {
        Ok(MockSentTx { output_amount: 0 })
    }
}

impl ViewContract for SimulationClient {
    async fn view_contract<T>(
        &self,
        receiver: &AccountId,
        method_name: &str,
        _args: &T,
    ) -> anyhow::Result<CallResult>
    where
        T: ?Sized + serde::Serialize + Sync,
    {
        let result = match method_name {
            "get_deposits" => {
                let state = self.portfolio.lock().await;
                let mut deposits = serde_json::Map::new();

                // cash balance as wrap.near
                let wnear_token = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
                deposits.insert(
                    wnear_token,
                    serde_json::Value::String(state.cash_balance.as_bigdecimal().to_string()),
                );

                // token holdings
                for (token_account, amount) in &state.holdings {
                    deposits.insert(
                        token_account.to_string(),
                        serde_json::Value::String(amount.smallest_units().to_string()),
                    );
                }

                serde_json::to_vec(&deposits)?
            }
            "ft_metadata" => {
                // Look up decimals for the specific token (receiver)
                let receiver_token = TokenAccount::from(receiver.clone());
                let decimals = trade::token_cache::get_cached_decimals(&receiver_token)
                    .or_else(|| {
                        // Fall back to holdings decimals.
                        // Uses try_lock (not lock) to avoid violating the sim_day→portfolio
                        // lock ordering documented on SimulationClient. view_contract is
                        // called without holding sim_day, so a blocking lock could deadlock
                        // if another task holds portfolio and waits for sim_day.
                        self.portfolio.try_lock().ok().and_then(|state| {
                            state.holdings.get(&receiver_token).map(|a| a.decimals())
                        })
                    })
                    .unwrap_or(DEFAULT_DECIMALS);
                let metadata = json!({
                    "spec": "ft-1.0.0",
                    "name": "SimToken",
                    "symbol": "SIM",
                    "decimals": decimals,
                });
                serde_json::to_vec(&metadata)?
            }
            "ft_balance_of" => {
                // Return large value to simulate sufficient liquidity
                let balance = U128(10u128.pow(30));
                serde_json::to_vec(&balance)?
            }
            "ft_total_supply" => {
                // Return large value
                let supply = U128(10u128.pow(30));
                serde_json::to_vec(&supply)?
            }
            "storage_balance_of" => {
                let account_info = json!({
                    "total": U128(100_000_000_000_000_000_000_000u128),
                    "available": U128(0),
                });
                serde_json::to_vec(&account_info)?
            }
            "storage_balance_bounds" => {
                let bounds = json!({
                    "min": U128(1_250_000_000_000_000_000_000u128),
                    "max": U128(1_250_000_000_000_000_000_000u128),
                });
                serde_json::to_vec(&bounds)?
            }
            _ => {
                let balance = U128(0);
                serde_json::to_vec(&balance)?
            }
        };

        Ok(CallResult {
            result,
            logs: vec![],
        })
    }
}

pub struct MockSentTx {
    output_amount: u128,
}

impl std::fmt::Display for MockSentTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockSentTx(sim, output={})", self.output_amount)
    }
}

impl SentTx for MockSentTx {
    async fn wait_for_executed(&self) -> anyhow::Result<FinalExecutionOutcomeViewEnum> {
        unimplemented!("SimulationClient does not execute real transactions")
    }

    async fn wait_for_success(&self) -> anyhow::Result<FinalExecutionOutcomeView> {
        let value_json = serde_json::to_vec(&U128(self.output_amount))?;
        Ok(blockchain::mock::dummy_final_outcome(value_json))
    }
}

#[cfg(test)]
mod tests;
