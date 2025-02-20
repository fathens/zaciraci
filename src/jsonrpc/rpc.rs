use crate::logging::*;
use near_jsonrpc_client::errors::{
    JsonRpcError, JsonRpcServerError, JsonRpcServerResponseStatusError, JsonRpcTransportSendError,
    RpcTransportError,
};
use near_jsonrpc_client::{methods, JsonRpcClient, MethodCallResult};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StandardRpcClient {
    underlying: Arc<JsonRpcClient>,

    retry_limit: u16,
    delay_limit: Duration,
    delay_fluctuation: f32,
}

impl StandardRpcClient {
    pub fn new(
        underlying: Arc<JsonRpcClient>,
        retry_limit: u16,
        delay_limit: Duration,
        delay_fluctuation: f32,
    ) -> Self {
        Self {
            underlying,
            retry_limit,
            delay_limit,
            delay_fluctuation,
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
            "server" => self.underlying.server_addr().to_owned(),
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
                    MaybeRetry::Retry {
                        err,
                        msg: "too many requests".to_owned(),
                        min_dur: Duration::from_secs_f32(0.5),
                    }
                }
                JsonRpcError::TransportError(RpcTransportError::SendError(
                    JsonRpcTransportSendError::PayloadSendError(e),
                )) => {
                    info!(log, "transport error: payload send error: {:?}", e);
                    MaybeRetry::Retry {
                        err: JsonRpcError::TransportError(RpcTransportError::SendError(
                            JsonRpcTransportSendError::PayloadSendError(e),
                        )),
                        msg: "payload send error".to_owned(),
                        min_dur: Duration::ZERO,
                    }
                }
                _ => {
                    info!(log, "failure");
                    MaybeRetry::Through(Err(err))
                }
            },
        }
    }
}

impl super::RpcClient for StandardRpcClient {
    fn server_addr(&self) -> &str {
        self.underlying.server_addr()
    }

    async fn call<M>(&self, method: M) -> MethodCallResult<M::Response, M::Error>
    where
        M: methods::RpcMethod,
    {
        let delay_limit = self.delay_limit;
        let retry_limit = self.retry_limit;
        let fluctuation = self.delay_fluctuation;

        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call",
            "server" => self.server_addr().to_owned(),
            "method" => method.method_name().to_owned(),
            "retry_limit" => format!("{}", retry_limit),
        ));
        let calc_delay = calc_retry_duration(delay_limit, retry_limit, fluctuation);
        let mut retry_count = 0;
        loop {
            let log = log.new(o!(
                "retry_count" => format!("{}", retry_count),
            ));
            info!(log, "calling");
            match self.call_maybe_retry(&method).await {
                MaybeRetry::Through(res) => return res,
                MaybeRetry::Retry { err, msg, min_dur } => {
                    retry_count += 1;
                    if retry_limit < retry_count {
                        info!(log, "retry limit reached";
                            "reason" => msg,
                        );
                        return Err(err);
                    }

                    let delay = calc_delay(retry_count).min(min_dur);
                    info!(log, "retrying";
                        "delay" => format!("{:?}", delay),
                        "reason" => msg,
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
}

enum MaybeRetry<A, B> {
    Through(A),
    Retry {
        err: B,
        msg: String,
        min_dur: Duration,
    },
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
