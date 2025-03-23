// ヘルスチェックサービスのgRPC実装

use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::generated::zaciraci::common::Empty;
use crate::generated::zaciraci::health::{HealthResponse, health_service_server::HealthService};
use crate::services::health::{HealthService as HealthServiceTrait, HealthServiceImpl};

// gRPCサービス実装
#[derive(Default)]
pub struct HealthServiceGrpc {
    service: Arc<HealthServiceImpl>,
}

impl HealthServiceGrpc {
    pub fn new(service: Arc<HealthServiceImpl>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl HealthService for HealthServiceGrpc {
    async fn healthcheck(&self, _request: Request<Empty>) -> Result<Response<HealthResponse>, Status> {
        // ビジネスロジック層を呼び出し
        match self.service.check_health() {
            Ok(status) => {
                let reply = HealthResponse {
                    status,
                };
                Ok(Response::new(reply))
            }
            Err(err) => {
                Err(Status::internal(format!("ヘルスチェックエラー: {}", err)))
            }
        }
    }
}

// 利便性のためのタイプエイリアス
pub type HealthServiceServer = crate::generated::zaciraci::health::health_service_server::HealthServiceServer<HealthServiceGrpc>;
