mod endpoint_pool;
mod near_client;
mod rpc;
mod sent_tx;

use crate::Result;
use crate::config;
use crate::jsonrpc::near_client::StandardNearClient;
use crate::jsonrpc::rpc::StandardRpcClient;
use crate::logging::*;
use crate::types::gas_price::GasPrice;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{MethodCallResult, methods};
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::action::Action;
use near_primitives::hash::CryptoHash;
use near_primitives::types::{Balance, BlockId};
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
    TxExecutionStatus,
};
use near_sdk::AccountId;
use once_cell::sync::Lazy;
use std::sync::Arc;

pub static IS_MAINNET: Lazy<bool> = Lazy::new(|| {
    let str = config::get("USE_MAINNET").unwrap_or_default();
    let log = DEFAULT.new(o!(
        "function" => "IS_MAINNET",
        "given_value" => format!("{}", str),
    ));
    let value = str.parse().unwrap_or_default();
    if value {
        info!(log, "Using mainnet");
    } else {
        info!(log, "Using testnet");
    }
    value
});

pub fn new_client() -> StandardNearClient<StandardRpcClient> {
    StandardNearClient::new(&Arc::new(StandardRpcClient::new(
        Arc::new(endpoint_pool::EndpointPool::new()),
        128,
        std::time::Duration::from_secs(60),
        0.1,
    )))
}

pub trait RpcClient {
    fn call<M>(
        &self,
        method: M,
    ) -> impl std::future::Future<Output = MethodCallResult<M::Response, M::Error>> + Send
    where
        M: methods::RpcMethod + std::marker::Send + std::marker::Sync,
        <M as methods::RpcMethod>::Response: std::marker::Send,
        <M as methods::RpcMethod>::Error: std::marker::Send;
}

pub trait BlockInfo {
    async fn get_recent_block(&self) -> Result<BlockView>;
}

pub trait GasInfo {
    async fn get_gas_price(&self, block: Option<BlockId>) -> Result<GasPrice>;
}

pub trait AccountInfo {
    async fn get_native_amount(&self, account: &AccountId) -> Result<Balance>;
}

pub trait AccessKeyInfo {
    async fn get_access_key_info(&self, signer: &InMemorySigner) -> Result<AccessKeyView>;
}

pub trait TxInfo {
    async fn wait_tx_result(
        &self,
        sender: &AccountId,
        tx_hash: &CryptoHash,
        wait_until: TxExecutionStatus,
    ) -> Result<RpcTransactionResponse>;
}

pub trait ViewContract {
    fn view_contract<T>(
        &self,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
    ) -> impl std::future::Future<Output = Result<CallResult>> + Send
    where
        T: ?Sized + serde::Serialize + std::marker::Sync;
}

pub trait SendTx {
    type Output: SentTx;

    async fn transfer_native_token(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        amount: Balance,
    ) -> Result<Self::Output>;

    async fn exec_contract<T>(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        method_name: &str,
        args: T,
        deposit: Balance,
    ) -> Result<Self::Output>
    where
        T: Sized + serde::Serialize;

    async fn send_tx(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        actions: Vec<Action>,
    ) -> Result<Self::Output>;
}

pub trait SentTx {
    async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum>;
    async fn wait_for_success(&self) -> Result<ExecutionOutcomeView>;
}
