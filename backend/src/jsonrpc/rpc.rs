use crate::logging::*;
use near_jsonrpc_client::errors::{
    JsonRpcError, JsonRpcServerError, JsonRpcServerResponseStatusError, JsonRpcTransportRecvError,
    JsonRpcTransportSendError, RpcTransportError,
};
use near_jsonrpc_client::{JsonRpcClient, MethodCallResult, methods};
use rand::Rng;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct StandardRpcClient {
    endpoint_pool: Arc<super::endpoint_pool::EndpointPool>,

    retry_limit: u16,
    delay_limit: Duration,
    delay_fluctuation: f32,
}

impl StandardRpcClient {
    pub fn new(
        endpoint_pool: Arc<super::endpoint_pool::EndpointPool>,
        retry_limit: u16,
        delay_limit: Duration,
        delay_fluctuation: f32,
    ) -> Self {
        Self {
            endpoint_pool,
            retry_limit,
            delay_limit,
            delay_fluctuation,
        }
    }

    /// Get a client for the current best endpoint
    ///
    /// Returns (client, url, max_retries) tuple
    fn get_client(&self) -> Option<(Arc<JsonRpcClient>, String, u32)> {
        let endpoint = self.endpoint_pool.next_endpoint()?;
        let client = Arc::new(Self::create_client_with_timeout(&endpoint.url));
        Some((client, endpoint.url.clone(), endpoint.max_retries))
    }

    /// Create a JsonRpcClient with HTTP timeout configured
    fn create_client_with_timeout(server_addr: &str) -> JsonRpcClient {
        use near_jsonrpc_client::JsonRpcClient;

        // Configure reqwest client with timeouts
        let mut headers = reqwest::header::HeaderMap::with_capacity(2);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let reqwest_client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30)) // Total request timeout: 30 seconds
            .connect_timeout(Duration::from_secs(10)) // Connection timeout: 10 seconds
            .build()
            .expect("Failed to build reqwest client");

        // Use JsonRpcClient::with() to create a connector with custom reqwest client
        JsonRpcClient::with(reqwest_client).connect(server_addr)
    }

    async fn call_maybe_retry<M>(
        &self,
        client: &JsonRpcClient,
        endpoint_url: &str,
        method: M,
    ) -> MaybeRetry<MethodCallResult<M::Response, M::Error>, JsonRpcError<M::Error>>
    where
        M: methods::RpcMethod + Send + Sync,
        <M as methods::RpcMethod>::Response: Send,
        <M as methods::RpcMethod>::Error: Send,
    {
        let log = DEFAULT.new(o!(
            "function" => "jsonrpc::Client::call_maybe_retry",
            "server" => endpoint_url.to_owned(),
            "method" => method.method_name().to_owned(),
        ));
        debug!(log, "calling");
        let res = client.call(method).await;
        match res {
            Ok(res) => {
                trace!(log, "success");
                MaybeRetry::Through(Ok(res))
            }
            Err(err) => match &err {
                JsonRpcError::ServerError(server_err) => match server_err {
                    JsonRpcServerError::ResponseStatusError(
                        JsonRpcServerResponseStatusError::TooManyRequests,
                    ) => {
                        debug!(log, "response status error: too many requests");
                        // Mark endpoint as failed and switch to next endpoint immediately
                        self.endpoint_pool.mark_failed(endpoint_url);
                        MaybeRetry::SwitchEndpoint {
                            err,
                            msg: "too many requests".to_owned(),
                            min_dur: Duration::from_secs(1),
                        }
                    }
                    JsonRpcServerError::RequestValidationError(validation_err) => {
                        debug!(log, "server error: request validation error"; "error" => format!("{:?}", validation_err));
                        MaybeRetry::Through(Err(err)) // リクエスト自体が無効なのでリトライしない
                    }
                    JsonRpcServerError::HandlerError(_) => {
                        debug!(log, "server error: handler error");
                        // ハンドラーエラーの内容によってはリトライ可能かもしれないが、
                        // 基本的にはアプリケーションロジックのエラーなのでリトライしない
                        MaybeRetry::Through(Err(err))
                    }
                    JsonRpcServerError::InternalError { info } => {
                        debug!(log, "server error: internal error"; "error_info" => format!("{:?}", info));
                        // サーバー側の一時的な問題である可能性が高いのでリトライする
                        MaybeRetry::Retry {
                            err,
                            msg: "server internal error".to_owned(),
                            min_dur: Duration::from_secs_f32(0.5),
                        }
                    }
                    JsonRpcServerError::NonContextualError(non_contextual_err) => {
                        debug!(log, "server error: non contextual error"; "error" => format!("{:?}", non_contextual_err));
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
                                debug!(log, "server error: unauthorized");
                                // 認証エラーはリトライしても解決しない
                                MaybeRetry::Through(Err(err))
                            }
                            _ => {
                                debug!(log, "server error: response status error (other)"; "status_error" => format!("{:?}", status_err));
                                // 5xx系のエラーはサーバー側の問題の可能性が高いのでリトライ
                                let status = match status_err {
                                    JsonRpcServerResponseStatusError::Unexpected { status } => {
                                        Some(status)
                                    }
                                    _ => None,
                                };

                                if let Some(status) = status
                                    && status.is_server_error()
                                {
                                    let msg = format!("server error: {}", status);
                                    return MaybeRetry::Retry {
                                        err,
                                        msg,
                                        min_dur: Duration::from_secs_f32(0.5),
                                    };
                                }
                                MaybeRetry::Through(Err(err))
                            }
                        }
                    }
                },
                JsonRpcError::TransportError(transport_err) => match transport_err {
                    RpcTransportError::SendError(send_err) => match send_err {
                        JsonRpcTransportSendError::PayloadSendError(e) => {
                            debug!(log, "transport error: payload send error"; "error" => format!("{:?}", e));
                            MaybeRetry::Retry {
                                err,
                                msg: "payload send error".to_owned(),
                                min_dur: Duration::from_secs_f32(0.5), // ネットワーク一時障害の可能性
                            }
                        }
                        JsonRpcTransportSendError::PayloadSerializeError(serialize_err) => {
                            debug!(log, "transport error: payload serialize error"; "error" => format!("{:?}", serialize_err));
                            // シリアライズエラーはクライアント側の問題なのでリトライしない
                            MaybeRetry::Through(Err(err))
                        }
                    },
                    RpcTransportError::RecvError(recv_err) => match recv_err {
                        JsonRpcTransportRecvError::UnexpectedServerResponse(response) => {
                            debug!(log, "transport error: unexpected server response"; "response" => format!("{:?}", response));
                            // 予期しないレスポンス形式はリトライしても解決しにくい
                            MaybeRetry::Through(Err(err))
                        }
                        JsonRpcTransportRecvError::PayloadRecvError(recv_error) => {
                            let msg = recv_error.to_string();
                            debug!(log, "transport error: payload receive error"; "error" => format!("{:?}", recv_error));
                            // ネットワーク一時障害の可能性が高い
                            MaybeRetry::Retry {
                                err,
                                msg,
                                min_dur: Duration::from_secs(1),
                            }
                        }
                        JsonRpcTransportRecvError::PayloadParseError(parse_error) => {
                            let msg = format!("{:?}", parse_error);
                            debug!(log, "transport error: payload parse error"; "error" => format!("{:?}", parse_error));
                            // サーバー負荷による一時的な不完全レスポンスの可能性
                            MaybeRetry::Retry {
                                err,
                                msg,
                                min_dur: Duration::from_secs(2),
                            }
                        }
                        JsonRpcTransportRecvError::ResponseParseError(response_parse_error) => {
                            let msg = response_parse_error.to_string();
                            debug!(log, "transport error: response parse error"; "error" => format!("{:?}", response_parse_error));
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
            "method" => method.method_name().to_owned(),
            "retry_limit" => format!("{}", retry_limit),
        ));
        let calc_delay = calc_retry_duration(delay_limit, retry_limit, fluctuation);
        let mut total_retry_count: u16 = 0;

        // Outer loop: endpoint selection
        loop {
            let Some((client, endpoint_url, max_retries)) = self.get_client() else {
                let log = log.new(o!("error" => "no_available_endpoints"));
                error!(log, "No available endpoints in pool");
                // Continue retrying - pool may reset failed endpoints
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            };

            let mut endpoint_retry_count: u32 = 0;

            // Inner loop: retry on same endpoint
            loop {
                let log = log.new(o!(
                    "total_retry_count" => format!("{}", total_retry_count),
                    "endpoint_retry_count" => format!("{}", endpoint_retry_count),
                    "max_retries" => format!("{}", max_retries),
                    "endpoint" => endpoint_url.clone(),
                ));
                info!(log, "calling");

                match self.call_maybe_retry(&client, &endpoint_url, &method).await {
                    MaybeRetry::Through(res) => return res,
                    MaybeRetry::Retry { err, msg, min_dur } => {
                        total_retry_count += 1;
                        if retry_limit < total_retry_count {
                            warn!(log, "global retry limit reached";
                                "reason" => msg,
                            );
                            return Err(err);
                        }

                        endpoint_retry_count += 1;
                        if endpoint_retry_count > max_retries {
                            // Switch to next endpoint
                            warn!(log, "endpoint max retries reached, switching endpoint";
                                "reason" => msg,
                            );
                            self.endpoint_pool.mark_failed(&endpoint_url);
                            break; // Break inner loop to select next endpoint
                        }

                        // Retry on same endpoint
                        let delay = calc_delay(total_retry_count).max(min_dur);
                        debug!(log, "retrying on same endpoint";
                            "delay" => format!("{:?}", delay),
                            "reason" => msg,
                        );
                        tokio::time::sleep(delay).await;
                    }
                    MaybeRetry::SwitchEndpoint { err, msg, min_dur } => {
                        // Switch to next endpoint immediately (e.g., TooManyRequests)
                        // Endpoint is already marked as failed in call_maybe_retry
                        total_retry_count += 1;
                        if retry_limit < total_retry_count {
                            warn!(log, "global retry limit reached";
                                "reason" => &msg,
                            );
                            return Err(err);
                        }

                        let delay = calc_delay(total_retry_count).max(min_dur);
                        warn!(log, "switching to next endpoint";
                            "delay" => format!("{:?}", delay),
                            "reason" => msg,
                        );
                        tokio::time::sleep(delay).await;
                        break; // Break inner loop to select next endpoint
                    }
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
    /// Switch to next endpoint immediately (e.g., TooManyRequests)
    /// The endpoint has already been marked as failed
    SwitchEndpoint {
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
mod tests;
