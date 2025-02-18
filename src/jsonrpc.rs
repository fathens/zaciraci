mod client;
mod delegate;

use crate::config;
use crate::jsonrpc::client::StandardClient;
use crate::jsonrpc::delegate::StandardDelegate;
use crate::logging::*;
use crate::types::gas_price::GasPrice;
use crate::Result;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::{methods, JsonRpcClient, MethodCallResult};
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::hash::CryptoHash;
use near_primitives::types::{Balance, BlockId};
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
    TxExecutionStatus,
};
use near_sdk::AccountId;
use once_cell::sync::Lazy;

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

fn new_jsonrpc_client() -> JsonRpcClient {
    if *IS_MAINNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    }
}

pub fn new_delegate(
) -> impl BlockInfo + GasInfo + AccountInfo + AccessKeyInfo + TxInfo + ViewContract + SendTx {
    StandardDelegate {
        client: StandardClient {
            underlying: new_jsonrpc_client(),
        },
    }
}

pub fn new_client() -> impl Client {
    StandardClient {
        underlying: new_jsonrpc_client(),
    }
}

pub trait Client {
    fn server_addr(&self) -> &str;

    async fn call<M: methods::RpcMethod>(
        &self,
        method: M,
    ) -> MethodCallResult<M::Response, M::Error>;
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
    async fn view_contract<T>(
        &self,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
    ) -> Result<CallResult>
    where
        T: ?Sized + serde::Serialize;
}

pub trait SendTx {
    async fn transfer_native_token(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        amount: Balance,
    ) -> Result<impl SentTx>;

    async fn exec_contract<T>(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
        deposit: Balance,
    ) -> Result<impl SentTx>
    where
        T: ?Sized + serde::Serialize;
}

pub trait SentTx {
    async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum>;
    async fn wait_for_success(&self) -> Result<ExecutionOutcomeView>;
}
