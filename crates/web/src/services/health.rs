use crate::proto::health_service_server::HealthService;
use crate::proto::{HealthCheckRequest, HealthCheckResponse};
use tonic::{Request, Response, Status};

pub struct HealthServiceImpl;

#[tonic::async_trait]
impl HealthService for HealthServiceImpl {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let db_status = match persistence::connection_pool::get().await {
            Ok(_) => "connected".to_string(),
            Err(e) => format!("error: {e}"),
        };
        let healthy = db_status == "connected";

        Ok(Response::new(HealthCheckResponse {
            healthy,
            database_status: db_status,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_returns_healthy_when_db_available() {
        let svc = HealthServiceImpl;
        let response = svc
            .check(Request::new(HealthCheckRequest {}))
            .await
            .unwrap();
        let resp = response.into_inner();
        assert!(resp.healthy);
        assert_eq!(resp.database_status, "connected");
    }
}
