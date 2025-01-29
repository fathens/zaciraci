use crate::jsonrpc::IS_MAINNET;
use crate::logging::*;
use near_jsonrpc_client::errors::{
    JsonRpcError, JsonRpcServerError, JsonRpcServerResponseStatusError, JsonRpcTransportSendError,
    RpcTransportError,
};
use near_jsonrpc_client::{methods, JsonRpcClient, MethodCallResult};
use once_cell::sync::Lazy;
use rand::Rng;
use std::time::Duration;

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
        const DELAY_LIMIT: Duration = Duration::from_secs(60);
        const RETRY_LIMIT: u16 = 128;
        const FLUCTUATION: f32 = 0.1;

        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call",
            "server" => self.server_addr().to_owned(),
            "method" => method.method_name().to_owned(),
            "retry_limit" => format!("{}", RETRY_LIMIT),
        ));
        let calc_delay = calc_retry_duration(DELAY_LIMIT, RETRY_LIMIT, FLUCTUATION);
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

                    let delay = calc_delay(retry_count);
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
            Err(err) => match err {
                JsonRpcError::ServerError(JsonRpcServerError::ResponseStatusError(
                    JsonRpcServerResponseStatusError::TooManyRequests,
                )) => {
                    info!(log, "response status error: too many requests");
                    MaybeRetry::Retry(err)
                }
                JsonRpcError::TransportError(RpcTransportError::SendError(
                    JsonRpcTransportSendError::PayloadSendError(e),
                )) => {
                    info!(log, "transport error: payload send error: {:?}", e);
                    MaybeRetry::Retry(JsonRpcError::TransportError(RpcTransportError::SendError(
                        JsonRpcTransportSendError::PayloadSendError(e),
                    )))
                }
                _ => {
                    info!(log, "failure");
                    MaybeRetry::Through(Err(err))
                }
            },
        }
    }
}

enum MaybeRetry<A, B> {
    Through(A),
    Retry(B),
}

fn calc_retry_duration(upper: Duration, retry_limit: u16, fr: f32) -> impl Fn(u16) -> Duration {
    const N: f32 = 1.0 / std::f32::consts::E;
    move |retry_count: u16| -> Duration {
        if retry_count == 0 || retry_limit < retry_count {
            return Duration::ZERO;
        }
        let b = (retry_count - 1) as f32 / (retry_limit - 1) as f32;
        let y = (upper.as_millis() as f32) / (1.0 / b).powf(N);
        let y = fluctuate(y, fr);
        Duration::from_millis(y as u64)
    }
}

fn fluctuate(y: f32, fr: f32) -> f32 {
    let r = y * fr;
    if r > 0.0 {
        let mut rng = rand::thread_rng();
        let v = rng.gen_range(0.0..(r * 2.0)) - r;
        y + v
    } else {
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertables::*;
    use proptest::prelude::*;

    #[test]
    fn test_calc_retry_duration() {
        let upper = Duration::from_secs(60);
        let limit = 128;
        let retry_dur = calc_retry_duration(upper, limit, 0.0);

        assert_eq!(retry_dur(0), Duration::ZERO);
        assert_eq!(retry_dur(1), Duration::ZERO);
        assert_eq!(retry_dur(limit), upper);
        assert_eq!(retry_dur(limit + 1), Duration::ZERO);
    }

    proptest! {
        #[test]
        fn test_calc_retry_duration_range(retry_count in 2u16..128) {
            let limit = 128u16;
            let upper = Duration::from_secs(128);
            let retry_dur = calc_retry_duration(upper, limit, 0.0);

            assert_gt!(retry_dur(retry_count), Duration::from_secs(retry_count as u64));
        }

        #[test]
        fn test_fluctuate_zero_y(fr in 0.0..1_f32) {
            let v = fluctuate(0.0, fr);
            assert_eq!(v, 0.0);
        }

        #[test]
        fn test_fluctuate_zero_fr(y in 0.0..1000_f32) {
            let v = fluctuate(y, 0.0);
            assert_eq!(v, y);
        }

        #[test]
        fn test_fluctuate(y in 1.0..1000_f32, fr in 0.01..1_f32) {
            let v = fluctuate(y, fr);
            assert_ge!(v, y - y * fr);
            assert_le!(v, y + y * fr);
        }
    }
}
