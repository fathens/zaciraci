use crate::portfolio_state::{
    self, DEFAULT_DECIMALS, PortfolioState, SwapEvent, SwapMethod, SwapResult, to_u128_or_warn,
};
use bigdecimal::{BigDecimal, RoundingMode};
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx, ViewContract};
use blockchain::ref_finance::swap::SwapAction;
use blockchain::types::gas_price::GasPrice;
use chrono::{DateTime, Utc};
use common::types::{TokenAccount, TokenAmount, YoctoValue};
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
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Simulated NEAR blockchain client for backtesting.
///
/// **Lock ordering**: When acquiring multiple locks, always lock `sim_day`
/// before `portfolio` to avoid deadlocks. This order must be consistent
/// across all call sites.
pub struct SimulationClient {
    portfolio: Arc<Mutex<PortfolioState>>,
    initial_native: YoctoValue,
    sim_day: Arc<Mutex<DateTime<Utc>>>,
    /// Tokens currently registered in the simulated REF Finance account,
    /// mirroring the on-chain `get_deposits` key set. Empty initially so
    /// the first `ensure_ref_storage_setup` takes the `InitialRegister`
    /// path (which bypasses the storage cap), matching production behavior
    /// on a fresh account.
    registered: Arc<Mutex<BTreeSet<TokenAccount>>>,
}

/// REF Finance `storage_balance_bounds.min` value used as the per-token
/// storage unit (matches the testnet/mainnet contract). Both the bounds
/// view and the `storage_balance_of.total` derivation depend on this
/// constant — share a single literal so the storage planner's per-token
/// assumption stays internally consistent.
const STORAGE_BOUND_MIN_YOCTO: u128 = 1_250_000_000_000_000_000_000;

/// On-chain `U128` values are always integer strings. Yocto cannot be
/// fractional, so any non-zero scale on a simulate-side `BigDecimal` is an
/// arithmetic artifact that must be truncated before mimicking the chain
/// response — otherwise downstream `U128` deserialization fails on the dot.
fn yocto_bigdecimal_to_u128_string(value: &BigDecimal) -> String {
    value.with_scale_round(0, RoundingMode::Down).to_string()
}

fn decimals_for(token: &TokenAccount) -> u8 {
    trade::token_cache::get_cached_decimals(token).unwrap_or_else(|| {
        let log = DEFAULT.new(o!("function" => "decimals_for"));
        warn!(log, "token decimals not cached, using default";
            "token" => %token, "default" => DEFAULT_DECIMALS);
        DEFAULT_DECIMALS
    })
}

impl SimulationClient {
    pub fn new(
        portfolio: Arc<Mutex<PortfolioState>>,
        initial_native: YoctoValue,
        sim_day: Arc<Mutex<DateTime<Utc>>>,
    ) -> Self {
        Self {
            portfolio,
            initial_native,
            sim_day,
            registered: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }

    /// Pre-populate the registered token set. Used by tests that bootstrap a
    /// portfolio with holdings (which logically implies the tokens are already
    /// deposited in REF Finance). Production simulate code reaches the same
    /// state organically via `register_tokens` / `ft_transfer_call`.
    #[cfg(test)]
    pub(crate) async fn pre_register(&self, tokens: impl IntoIterator<Item = TokenAccount>) {
        self.registered.lock().await.extend(tokens);
    }
}

fn parse_token_ids(args_value: &serde_json::Value) -> Vec<TokenAccount> {
    let Some(ids) = args_value.get("token_ids").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    ids.iter()
        .filter_map(|v| v.as_str()?.parse().ok())
        .collect()
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
            .array_windows::<2>()
            .all(|[a, b]| a.token_out == b.token_in),
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
        let Some(first) = swap_actions.first() else {
            return Ok(0);
        };
        // Safety: last() is always Some when first() is Some (non-empty slice).
        let last = swap_actions.last().expect("non-empty after first() check");
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
        Ok(NearToken::from_yoctonear(to_u128_or_warn(
            self.initial_native.as_bigdecimal(),
            "initial_native",
        )))
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
        receiver: &AccountId,
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

        // The mock has to model the on-chain "registered tokens" set so that
        // the production storage planner sees a faithful `get_deposits` shape.
        // Without this, a fresh simulation would either spuriously hit the
        // `Normal` cap-checked path (and reject 10-token registration) or
        // skip registration entirely, both of which diverge from production.
        let ref_contract = &*blockchain::ref_finance::CONTRACT_ADDRESS;
        match method_name {
            "register_tokens" if receiver == ref_contract => {
                let args_value = serde_json::to_value(&args)?;
                let tokens = parse_token_ids(&args_value);
                self.registered.lock().await.extend(tokens);
            }
            "unregister_tokens" if receiver == ref_contract => {
                let args_value = serde_json::to_value(&args)?;
                let tokens = parse_token_ids(&args_value);
                let mut set = self.registered.lock().await;
                for token in &tokens {
                    set.remove(token);
                }
            }
            "ft_transfer_call" => {
                // Depositing a token to REF (`receiver_id == REF`) implicitly
                // registers it on the user's REF account — production never
                // calls `register_tokens` for wnear before the initial deposit.
                let args_value = serde_json::to_value(&args)?;
                let target = args_value
                    .get("receiver_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<AccountId>().ok());
                if target.as_ref() == Some(ref_contract) {
                    let token = TokenAccount::from(receiver.clone());
                    self.registered.lock().await.insert(token);
                }
            }
            "storage_deposit" if receiver == ref_contract => {
                // No-op: account-level storage deposit doesn't add tokens to
                // `get_deposits`; the mock's `storage_balance_of` already
                // returns a registered account state.
            }
            _ => {}
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
                // Production `get_deposits` returns exactly the tokens the
                // account is registered for, with their current balance
                // (zero entries are kept until `unregister_tokens`). The mock
                // mirrors that: only `registered` tokens appear, with cash
                // balance for wnear and `holdings` for everything else.
                let registered = self.registered.lock().await;
                let state = self.portfolio.lock().await;
                let mut deposits = serde_json::Map::new();

                let wnear_account = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
                for token in registered.iter() {
                    let amount = if token == wnear_account {
                        yocto_bigdecimal_to_u128_string(state.cash_balance.as_bigdecimal())
                    } else {
                        match state.holdings.get(token) {
                            Some(holding) => {
                                yocto_bigdecimal_to_u128_string(holding.smallest_units())
                            }
                            None => "0".to_string(),
                        }
                    };
                    deposits.insert(token.to_string(), serde_json::Value::String(amount));
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
                        // lock ordering documented on SimulationClient.
                        match self.portfolio.try_lock() {
                            Ok(state) => state.holdings.get(&receiver_token).map(|a| a.decimals()),
                            Err(_) => {
                                // When try_lock fails, use decimals_for() (token_cache lookup)
                                // instead of DEFAULT_DECIMALS. This avoids 10^18 magnitude
                                // errors when DEFAULT_DECIMALS(24) is applied to 6-decimal
                                // tokens like USDT.
                                let log = DEFAULT.new(o!("function" => "ft_metadata"));
                                warn!(log, "portfolio try_lock failed, falling back to token_cache";
                                    "token" => %receiver_token);
                                Some(decimals_for(&receiver_token))
                            }
                        }
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
                // The on-chain `storage_balance_of.total` scales with the number
                // of registered tokens (each adds at least `bounds.min` worth of
                // storage cost; the account header itself also costs `bounds.min`).
                // A static total ignores the registered count and inflates
                // `per_token` in the storage planner, which then mis-estimates
                // top-up at cap-check time and blocks `register_tokens`. Mirror
                // production's accounting by deriving total from `registered.len()`.
                let registered = self.registered.lock().await;
                let account_slots =
                    u128::try_from(registered.len().saturating_add(1)).unwrap_or(u128::MAX);
                let total = STORAGE_BOUND_MIN_YOCTO.saturating_mul(account_slots);
                let account_info = json!({
                    "total": U128(total),
                    "available": U128(0),
                });
                serde_json::to_vec(&account_info)?
            }
            "storage_balance_bounds" => {
                let bounds = json!({
                    "min": U128(STORAGE_BOUND_MIN_YOCTO),
                    "max": U128(STORAGE_BOUND_MIN_YOCTO),
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
