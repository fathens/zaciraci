use crate::logging::*;
use near_jsonrpc_client::errors::{
    JsonRpcError, JsonRpcServerError, JsonRpcServerResponseStatusError, JsonRpcTransportRecvError,
    JsonRpcTransportSendError, RpcTransportError,
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
        M: methods::RpcMethod + Send + Sync,
        <M as methods::RpcMethod>::Response: Send,
        <M as methods::RpcMethod>::Error: Send,
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
            Err(err) => match &err {
                JsonRpcError::ServerError(server_err) => match server_err {
                    JsonRpcServerError::ResponseStatusError(
                        JsonRpcServerResponseStatusError::TooManyRequests,
                    ) => {
                        info!(log, "response status error: too many requests");
                        MaybeRetry::Retry {
                            err,
                            msg: "too many requests".to_owned(),
                            min_dur: Duration::from_secs(1),
                        }
                    }
                    JsonRpcServerError::RequestValidationError(validation_err) => {
                        info!(log, "server error: request validation error"; "error" => format!("{:?}", validation_err));
                        MaybeRetry::Through(Err(err)) // リクエスト自体が無効なのでリトライしない
                    }
                    JsonRpcServerError::HandlerError(_) => {
                        info!(log, "server error: handler error");
                        // ハンドラーエラーの内容によってはリトライ可能かもしれないが、
                        // 基本的にはアプリケーションロジックのエラーなのでリトライしない
                        MaybeRetry::Through(Err(err))
                    }
                    JsonRpcServerError::InternalError { info } => {
                        info!(log, "server error: internal error"; "error_info" => format!("{:?}", info));
                        // サーバー側の一時的な問題である可能性が高いのでリトライする
                        MaybeRetry::Retry {
                            err,
                            msg: "server internal error".to_owned(),
                            min_dur: Duration::from_secs_f32(0.5),
                        }
                    }
                    JsonRpcServerError::NonContextualError(non_contextual_err) => {
                        info!(log, "server error: non contextual error"; "error" => format!("{:?}", non_contextual_err));
                        // 詳細不明なエラーだが、サーバー側の問題の可能性もあるので慎重にリトライ
                        MaybeRetry::Retry {
                            err,
                            msg: "non contextual error".to_owned(),
                            min_dur: Duration::from_secs_f32(0.5),
                        }
                    }
                    JsonRpcServerError::ResponseStatusError(status_err) => {
                        match status_err {
                            JsonRpcServerResponseStatusError::Unauthorized => {
                                info!(log, "server error: unauthorized");
                                // 認証エラーはリトライしても解決しない
                                MaybeRetry::Through(Err(err))
                            }
                            _ => {
                                info!(log, "server error: response status error (other)"; "status_error" => format!("{:?}", status_err));
                                // 5xx系のエラーはサーバー側の問題の可能性が高いのでリトライ
                                let status = match status_err {
                                    JsonRpcServerResponseStatusError::Unexpected { status } => {
                                        Some(status)
                                    }
                                    _ => None,
                                };

                                if let Some(status) = status {
                                    if status.is_server_error() {
                                        let msg = format!("server error: {}", status);
                                        return MaybeRetry::Retry {
                                            err,
                                            msg,
                                            min_dur: Duration::from_secs_f32(0.5),
                                        };
                                    }
                                }
                                MaybeRetry::Through(Err(err))
                            }
                        }
                    }
                },
                JsonRpcError::TransportError(transport_err) => match transport_err {
                    RpcTransportError::SendError(send_err) => match send_err {
                        JsonRpcTransportSendError::PayloadSendError(e) => {
                            info!(log, "transport error: payload send error"; "error" => format!("{:?}", e));
                            MaybeRetry::Retry {
                                err,
                                msg: "payload send error".to_owned(),
                                min_dur: Duration::from_secs_f32(0.5), // ネットワーク一時障害の可能性
                            }
                        }
                        JsonRpcTransportSendError::PayloadSerializeError(serialize_err) => {
                            info!(log, "transport error: payload serialize error"; "error" => format!("{:?}", serialize_err));
                            // シリアライズエラーはクライアント側の問題なのでリトライしない
                            MaybeRetry::Through(Err(err))
                        }
                    },
                    RpcTransportError::RecvError(recv_err) => match recv_err {
                        JsonRpcTransportRecvError::UnexpectedServerResponse(response) => {
                            info!(log, "transport error: unexpected server response"; "response" => format!("{:?}", response));
                            // 予期しないレスポンス形式はリトライしても解決しにくい
                            MaybeRetry::Through(Err(err))
                        }
                        JsonRpcTransportRecvError::PayloadRecvError(recv_error) => {
                            let msg = recv_error.to_string();
                            info!(log, "transport error: payload receive error"; "error" => format!("{:?}", recv_error));
                            // ネットワーク一時障害の可能性が高い
                            MaybeRetry::Retry {
                                err,
                                msg,
                                min_dur: Duration::from_secs(1),
                            }
                        }
                        JsonRpcTransportRecvError::PayloadParseError(parse_error) => {
                            let msg = format!("{:?}", parse_error);
                            info!(log, "transport error: payload parse error"; "error" => format!("{:?}", parse_error));
                            // サーバー負荷による一時的な不完全レスポンスの可能性
                            MaybeRetry::Retry {
                                err,
                                msg,
                                min_dur: Duration::from_secs(2),
                            }
                        }
                        JsonRpcTransportRecvError::ResponseParseError(response_parse_error) => {
                            let msg = response_parse_error.to_string();
                            info!(log, "transport error: response parse error"; "error" => format!("{:?}", response_parse_error));
                            // サーバー側の一時的な問題の可能性
                            MaybeRetry::Retry {
                                err,
                                msg,
                                min_dur: Duration::from_secs_f32(0.5),
                            }
                        }
                    },
                },
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
        M: methods::RpcMethod + Send + Sync,
        <M as methods::RpcMethod>::Response: Send,
        <M as methods::RpcMethod>::Error: Send,
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
        let mut rng = rand::rng();
        let v = rng.random_range(0.0..(r * 2.0)) - r;
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
