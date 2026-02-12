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
