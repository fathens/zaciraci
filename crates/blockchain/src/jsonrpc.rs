mod endpoint_pool;
mod near_client;
mod rpc;
mod sent_tx;

#[cfg(test)]
mod near_compat_tests;

use crate::Result;
use crate::jsonrpc::near_client::StandardNearClient;
use crate::jsonrpc::rpc::StandardRpcClient;
use crate::types::gas_price::GasPrice;
use common::config::ConfigAccess;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{MethodCallResult, methods};
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::action::Action;
use near_primitives::hash::CryptoHash;
use near_primitives::types::BlockId;
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, FinalExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
    TxExecutionStatus,
};
use near_sdk::{AccountId, NearToken};
use std::sync::{Arc, LazyLock};

/// 全呼び出し元で共有される EndpointPool
/// 障害エンドポイント情報を共有し、無駄なリトライを削減する
static SHARED_ENDPOINT_POOL: LazyLock<Arc<endpoint_pool::EndpointPool>> = LazyLock::new(|| {
    Arc::new(endpoint_pool::EndpointPool::new(
        common::config::startup::get(),
    ))
});

pub fn new_client() -> StandardNearClient<StandardRpcClient> {
    let retry_limit = common::config::typed().rpc_max_attempts();
    StandardNearClient::new(&Arc::new(StandardRpcClient::new(
        Arc::clone(&SHARED_ENDPOINT_POOL),
        retry_limit,
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
    async fn get_native_amount(&self, account: &AccountId) -> Result<NearToken>;
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
        amount: NearToken,
    ) -> Result<Self::Output>;

    async fn exec_contract<T>(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        method_name: &str,
        args: T,
        deposit: NearToken,
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
    async fn wait_for_success(&self) -> Result<FinalExecutionOutcomeView>;
}
