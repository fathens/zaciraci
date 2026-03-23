use crate::portfolio_state::{self, PortfolioState};
use bigdecimal::BigDecimal;
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx, ViewContract};
use blockchain::ref_finance::swap::SwapAction;
use blockchain::types::gas_price::GasPrice;
use chrono::{DateTime, Utc};
use common::types::TokenAccount;
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

pub struct SimulationClient {
    portfolio: Arc<Mutex<PortfolioState>>,
    initial_native: u128,
    sim_day: Arc<Mutex<DateTime<Utc>>>,
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

/// Walk SwapAction hops through pool estimate_return (pure calculation, no I/O).
fn estimate_swap_via_pools(
    pools: &dex::PoolInfoList,
    swap_actions: &[SwapAction],
    amount_in: u128,
) -> Option<u128> {
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
    ) -> Option<u128> {
        let sim_day = *self.sim_day.lock().await;
        let pools = persistence::pool_info::read_from_db(Some(sim_day.naive_utc()))
            .await
            .ok()?;
        estimate_swap_via_pools(&pools, swap_actions, amount_in)
    }

    /// Fallback: calculate swap output using DB rates (no fee/slippage).
    async fn calculate_swap_output_via_rates(
        &self,
        token_in: &str,
        amount_in: u128,
        token_out: &str,
    ) -> u128 {
        use common::types::{TokenAmount, TokenOutAccount, YoctoValue};
        use num_traits::ToPrimitive;

        let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
        let wnear_in = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();
        let sim_day = *self.sim_day.lock().await;

        // token_in -> NEAR value
        let near_value = if token_in == wnear_str {
            YoctoValue::from_yocto(BigDecimal::from(amount_in)).to_near()
        } else {
            let token_in_out: TokenOutAccount = match token_in.parse() {
                Ok(t) => t,
                Err(_) => return 0,
            };

            let rate =
                match portfolio_state::get_rate_at_date(&token_in_out, &wnear_in, sim_day).await {
                    Some(r) => r,
                    None => return 0,
                };

            let token_in_account: TokenAccount = match token_in.parse() {
                Ok(t) => t,
                Err(_) => return 0,
            };
            let decimals_in =
                trade::token_cache::get_cached_decimals(&token_in_account).unwrap_or(24);
            let token_amount =
                TokenAmount::from_smallest_units(BigDecimal::from(amount_in), decimals_in);
            &token_amount / &rate
        };

        // NEAR value -> token_out amount
        if token_out == wnear_str {
            near_value.to_yocto().as_bigdecimal().to_u128().unwrap_or(0)
        } else {
            let token_out_account: TokenOutAccount = match token_out.parse() {
                Ok(t) => t,
                Err(_) => return 0,
            };

            let rate =
                match portfolio_state::get_rate_at_date(&token_out_account, &wnear_in, sim_day)
                    .await
                {
                    Some(r) => r,
                    None => return 0,
                };

            let token_amount = &near_value * &rate;
            token_amount.smallest_units().to_u128().unwrap_or(0)
        }
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
        let log = DEFAULT.new(o!("function" => "SimulationClient::exec_contract"));

        if method_name == "swap" {
            // Parse swap actions from args
            let args_value = serde_json::to_value(&args)?;
            if let Some(actions_array) = args_value.get("actions") {
                let swap_actions: Vec<SwapAction> = serde_json::from_value(actions_array.clone())?;

                if !swap_actions.is_empty() {
                    let first = &swap_actions[0];
                    let last = &swap_actions[swap_actions.len() - 1];

                    let token_in = first.token_in.to_string();
                    let amount_in = first.amount_in.map(|a| a.0).unwrap_or(0);
                    let token_out = last.token_out.to_string();

                    if amount_in > 0 {
                        // Try pool-based estimate_return first (fee + slippage aware)
                        let amount_out = match self
                            .calculate_swap_output_via_pools(&swap_actions, amount_in)
                            .await
                        {
                            Some(out) => out,
                            None => {
                                // Fallback to DB rate conversion (no fee/slippage)
                                warn!(log, "pool data unavailable, falling back to DB rate";
                                    "token_in" => &token_in, "token_out" => &token_out
                                );
                                self.calculate_swap_output_via_rates(
                                    &token_in, amount_in, &token_out,
                                )
                                .await
                            }
                        };

                        if amount_out > 0 {
                            let token_in_account: TokenAccount = token_in.parse()?;
                            let token_out_account: TokenAccount = token_out.parse()?;
                            let mut state = self.portfolio.lock().await;
                            state.execute_simulated_swap(
                                &token_in_account,
                                amount_in,
                                &token_out_account,
                                amount_out,
                            );

                            trace!(log, "simulated swap";
                                "token_in" => &token_in_account.to_string(),
                                "amount_in" => amount_in,
                                "token_out" => &token_out_account.to_string(),
                                "amount_out" => amount_out
                            );

                            return Ok(MockSentTx {
                                output_amount: amount_out,
                            });
                        } else {
                            warn!(log, "swap output is zero, skipping";
                                "token_in" => &token_in, "token_out" => &token_out
                            );
                        }
                    }
                }
            }
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
                        // Fall back to holdings decimals
                        self.portfolio.try_lock().ok().and_then(|state| {
                            state.holdings.get(&receiver_token).map(|a| a.decimals())
                        })
                    })
                    .unwrap_or(24);
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
