use crate::jsonrpc::IS_MAINNET;
use crate::logging::*;
use near_jsonrpc_client::errors::{
    JsonRpcError, JsonRpcServerError, JsonRpcServerResponseStatusError,
};
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
        const DELAY_LIMIT: u64 = 60_000;
        const RETRY_LIMIT: u8 = 10;

        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call",
            "server" => self.server_addr().to_owned(),
            "method" => method.method_name().to_owned(),
            "retry_limit" => format!("{}", RETRY_LIMIT),
        ));
        let mut retry_count = 0;
        loop {
            let log = log.new(o!(
                "retry_count" => format!("{}", retry_count),
            ));
            info!(log, "calling");
            match self.call_maybe_retry(&method).await {
                MaybeRetry::Through(res) => return res,
                MaybeRetry::Retry(err) => {
                    retry_count += 1;
                    if RETRY_LIMIT < retry_count {
                        info!(log, "retry limit reached");
                        return Err(err);
                    }

                    let delay = std::time::Duration::from_millis({
                        let b = (RETRY_LIMIT as f32) / (retry_count as f32);
                        let y = (DELAY_LIMIT as f32) / b.sqrt();
                        y as u64
                    });
                    info!(log, "retrying";
                        "delay" => format!("{:?}", delay),
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    async fn call_maybe_retry<M>(
        &self,
        method: M,
    ) -> MaybeRetry<MethodCallResult<M::Response, M::Error>, JsonRpcError<M::Error>>
    where
        M: methods::RpcMethod,
    {
        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call_maybe_retry",
            "server" => self.server_addr().to_owned(),
            "method" => method.method_name().to_owned(),
        ));
        info!(log, "calling");
        let res = self.underlying.call(method).await;
        match res {
            Ok(res) => {
                info!(log, "success");
                MaybeRetry::Through(Ok(res))
            }
            Err(err) => {
                if let JsonRpcError::ServerError(JsonRpcServerError::ResponseStatusError(
                    JsonRpcServerResponseStatusError::TooManyRequests,
                )) = err
                {
                    info!(log, "response status error: too many requests");
                    MaybeRetry::Retry(err)
                } else {
                    info!(log, "failure");
                    MaybeRetry::Through(Err(err))
                }
            }
        }
    }
}

enum MaybeRetry<A, B> {
    Through(A),
    Retry(B),
}
