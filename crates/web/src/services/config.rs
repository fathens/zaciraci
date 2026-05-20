use crate::proto::config_service_server::ConfigService;
use crate::proto::{
    ConfigEntry, DeleteConfigRequest, DeleteConfigResponse, GetAllConfigRequest,
    GetAllConfigResponse, GetOneConfigRequest, GetOneConfigResponse, KeyDefinitionEntry,
    ListKeyDefinitionsRequest, ListKeyDefinitionsResponse, UpsertConfigRequest,
    UpsertConfigResponse,
};
use crate::services::auth::{require_reader, require_writer};
use common::config::ConfigAccess;
use logging::{DEFAULT, o, warn};
use tonic::{Request, Response, Status};

impl From<common::config::ConfigValueType> for crate::proto::ConfigValueType {
    fn from(vt: common::config::ConfigValueType) -> Self {
        use common::config::ConfigValueType as CVT;
        match vt {
            CVT::Bool => Self::Bool,
            CVT::U16 => Self::U16,
            CVT::U32 => Self::U32,
            CVT::U64 => Self::U64,
            CVT::U128 => Self::U128,
            CVT::I64 => Self::I64,
            CVT::F64 => Self::F64,
            CVT::String => Self::String,
            CVT::Duration => Self::Duration,
        }
    }
}

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

/// 古い config_store_history レコードをバックグラウンドでクリーンアップ
fn spawn_cleanup_old_config_history() {
    tokio::spawn(async {
        let retention_days = common::config::typed().config_store_history_retention_days();
        if let Err(e) = persistence::config_store::cleanup_old_history(retention_days).await {
            let log = DEFAULT.new(o!("function" => "cleanup_config_history"));
            warn!(log, "failed to cleanup old config history"; "error" => %e);
        }
    });
}

#[tonic::async_trait]
impl ConfigService for ConfigServiceImpl {
    async fn get_all(
        &self,
        request: Request<GetAllConfigRequest>,
    ) -> Result<Response<GetAllConfigResponse>, Status> {
        require_reader(&request)?;

        let instance_id = Self::resolve_instance_id(&request.get_ref().instance_id);

        let configs = persistence::config_store::get_all_for_instance(instance_id)
            .await
            .map_err(|e| {
                let log = DEFAULT.new(o!("function" => "get_all"));
                warn!(log, "failed to get config"; "error" => %e);
                Status::internal("internal error")
            })?;

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
        require_reader(&request)?;

        let req = request.get_ref();
        let instance_id = Self::resolve_instance_id(&req.instance_id);

        if req.key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        let value = persistence::config_store::get_one(instance_id, &req.key)
            .await
            .map_err(|e| {
                let log = DEFAULT.new(o!("function" => "get_one"));
                warn!(log, "failed to get config"; "error" => %e);
                Status::internal("internal error")
            })?;

        Ok(Response::new(GetOneConfigResponse { value }))
    }

    async fn upsert(
        &self,
        request: Request<UpsertConfigRequest>,
    ) -> Result<Response<UpsertConfigResponse>, Status> {
        require_writer(&request)?;

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
        .map_err(|e| {
            let log = DEFAULT.new(o!("function" => "upsert"));
            warn!(log, "failed to upsert config"; "error" => %e);
            Status::internal("internal error")
        })?;

        spawn_cleanup_old_config_history();

        Ok(Response::new(UpsertConfigResponse {}))
    }

    async fn delete(
        &self,
        request: Request<DeleteConfigRequest>,
    ) -> Result<Response<DeleteConfigResponse>, Status> {
        require_writer(&request)?;

        let req = request.get_ref();
        let instance_id = Self::resolve_instance_id(&req.instance_id);

        if req.key.is_empty() {
            return Err(Status::invalid_argument("key must not be empty"));
        }

        persistence::config_store::delete(instance_id, &req.key)
            .await
            .map_err(|e| {
                let log = DEFAULT.new(o!("function" => "delete"));
                warn!(log, "failed to delete config"; "error" => %e);
                Status::internal("internal error")
            })?;

        spawn_cleanup_old_config_history();

        Ok(Response::new(DeleteConfigResponse {}))
    }

    async fn list_key_definitions(
        &self,
        request: Request<ListKeyDefinitionsRequest>,
    ) -> Result<Response<ListKeyDefinitionsResponse>, Status> {
        require_reader(&request)?;

        let resolved = common::config::resolve_all_without_db();
        let definitions = resolved
            .into_iter()
            .map(|info| KeyDefinitionEntry {
                key: info.key,
                description: info.description.trim().to_string(),
                value_type: crate::proto::ConfigValueType::from(info.value_type).into(),
                resolved_value: info.resolved_value,
            })
            .collect();
        Ok(Response::new(ListKeyDefinitionsResponse { definitions }))
    }
}

#[cfg(test)]
mod tests;
