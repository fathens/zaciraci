use crate::jsonrpc::IS_MAINNET;
use crate::logging::*;
use near_jsonrpc_client::{methods, JsonRpcClient, MethodCallResult};
use once_cell::sync::Lazy;

pub static CLIENT: Lazy<Client> = Lazy::new(|| {
    let underlying = if *IS_MAINNET {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_MAINNET_RPC_URL)
    } else {
        JsonRpcClient::connect(near_jsonrpc_client::NEAR_TESTNET_RPC_URL)
    };
    Client::new(underlying)
});

pub struct Client {
    underlying: JsonRpcClient,
}

impl Client {
    pub fn new(underlying: JsonRpcClient) -> Self {
        Self { underlying }
    }

    pub fn server_addr(&self) -> &str {
        self.underlying.server_addr()
    }

    pub async fn call<M>(&self, method: M) -> MethodCallResult<M::Response, M::Error>
    where
        M: methods::RpcMethod,
    {
        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call",
            "server" => self.server_addr().to_owned(),
            "method" => method.method_name().to_owned(),
        ));
        debug!(log, "calling");
        let res = self.underlying.call(method).await;
        match res {
            Ok(res) => {
                debug!(log, "success");
                Ok(res)
            }
            Err(err) => {
                warn!(log, "failure");
                Err(err)
            }
        }
    }
}
