use crate::proto::config_service_server::ConfigService;
use crate::proto::{
    ConfigEntry, DeleteConfigRequest, DeleteConfigResponse, GetAllConfigRequest,
    GetAllConfigResponse, GetOneConfigRequest, GetOneConfigResponse, UpsertConfigRequest,
    UpsertConfigResponse,
};
use tonic::{Request, Response, Status};

pub struct ConfigServiceImpl;

impl ConfigServiceImpl {
    fn resolve_instance_id(instance_id: &str) -> &str {
        if instance_id.is_empty() {
            "*"
        } else {
            instance_id
        }
    }
}

#[tonic::async_trait]
impl ConfigService for ConfigServiceImpl {
    async fn get_all(
        &self,
        request: Request<GetAllConfigRequest>,
    ) -> Result<Response<GetAllConfigResponse>, Status> {
        let instance_id = Self::resolve_instance_id(&request.get_ref().instance_id);

        let configs = persistence::config_store::get_all_for_instance(instance_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to get config: {e}")))?;

        let entries = configs
            .into_iter()
            .map(|(key, value)| ConfigEntry {
                key,
                value,
                instance_id: instance_id.to_string(),
            })
            .collect();

        Ok(Response::new(GetAllConfigResponse { entries }))
    }

    async fn get_one(
        &self,
        request: Request<GetOneConfigRequest>,
    ) -> Result<Response<GetOneConfigResponse>, Status> {
        let req = request.get_ref();
        let instance_id = Self::resolve_instance_id(&req.instance_id);

        if req.key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        let value = persistence::config_store::get_one(instance_id, &req.key)
            .await
            .map_err(|e| Status::internal(format!("Failed to get config: {e}")))?;

        Ok(Response::new(GetOneConfigResponse { value }))
    }

    async fn upsert(
        &self,
        request: Request<UpsertConfigRequest>,
    ) -> Result<Response<UpsertConfigResponse>, Status> {
        let req = request.get_ref();
        let instance_id = Self::resolve_instance_id(&req.instance_id);

        if req.key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        persistence::config_store::upsert(
            instance_id,
            &req.key,
            &req.value,
            req.description.as_deref(),
        )
        .await
        .map_err(|e| Status::internal(format!("Failed to upsert config: {e}")))?;

        Ok(Response::new(UpsertConfigResponse {}))
    }

    async fn delete(
        &self,
        request: Request<DeleteConfigRequest>,
    ) -> Result<Response<DeleteConfigResponse>, Status> {
        let req = request.get_ref();
        let instance_id = Self::resolve_instance_id(&req.instance_id);

        if req.key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        persistence::config_store::delete(instance_id, &req.key)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete config: {e}")))?;

        Ok(Response::new(DeleteConfigResponse {}))
    }
}
