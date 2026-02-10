use crate::portfolio_state::PortfolioState;
use blockchain::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx, ViewContract};
use blockchain::types::gas_price::GasPrice;
use near_crypto::InMemorySigner;
use near_primitives::action::Action;
use near_primitives::types::BlockId;
use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct SimulationClient {
    portfolio: Arc<Mutex<PortfolioState>>,
    initial_native: u128,
}

impl SimulationClient {
    pub fn new(portfolio: Arc<Mutex<PortfolioState>>, initial_native: u128) -> Self {
        Self {
            portfolio,
            initial_native,
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
        Ok(MockSentTx)
    }

    async fn exec_contract<T>(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _method_name: &str,
        _args: T,
        _deposit: NearToken,
    ) -> anyhow::Result<Self::Output>
    where
        T: Sized + serde::Serialize,
    {
        Ok(MockSentTx)
    }

    async fn send_tx(
        &self,
        _signer: &InMemorySigner,
        _receiver: &AccountId,
        _actions: Vec<Action>,
    ) -> anyhow::Result<Self::Output> {
        Ok(MockSentTx)
    }
}

impl ViewContract for SimulationClient {
    async fn view_contract<T>(
        &self,
        _receiver: &AccountId,
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
                    serde_json::Value::String(state.cash_balance.to_string()),
                );

                // token holdings
                for (token_id, amount) in &state.holdings {
                    deposits.insert(
                        token_id.clone(),
                        serde_json::Value::String(amount.to_string()),
                    );
                }

                serde_json::to_vec(&deposits)?
            }
            "ft_metadata" => {
                let state = self.portfolio.lock().await;
                // Try to find the token decimals from portfolio state
                // Default to 24 if not found
                let decimals = state.decimals.values().next().copied().unwrap_or(24);
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

#[cfg(test)]
mod tests;

pub struct MockSentTx;

impl std::fmt::Display for MockSentTx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockSentTx(sim)")
    }
}

impl SentTx for MockSentTx {
    async fn wait_for_executed(&self) -> anyhow::Result<FinalExecutionOutcomeViewEnum> {
        unimplemented!("SimulationClient does not execute real transactions")
    }

    async fn wait_for_success(&self) -> anyhow::Result<ExecutionOutcomeView> {
        Ok(ExecutionOutcomeView {
            logs: vec![],
            receipt_ids: vec![],
            gas_burnt: near_primitives::types::Gas::from_gas(0),
            tokens_burnt: NearToken::from_yoctonear(0),
            executor_id: AccountId::try_from("sim.near".to_string())?,
            status: near_primitives::views::ExecutionStatusView::SuccessValue(vec![]),
            metadata: near_primitives::views::ExecutionMetadataView {
                version: 1,
                gas_profile: None,
            },
        })
    }
}
