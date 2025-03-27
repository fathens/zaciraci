use super::AppState;
use crate::logging::*;
use crate::ollama::{self, Message};
use axum::{
    Router,
    extract::{Json, Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct ChatRequest {
    model_name: String,
    role: String,
    content: String,
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route("/model_names", get(list_model_names))
        .route("/chat/{port}", post(chat))
}

async fn list_model_names() -> String {
    let log = DEFAULT.new(o!(
        "function" => "list_model_names"
    ));
    let models = match ollama::list_models().await {
        Ok(models) => models,
        Err(err) => {
            info!(log, "Failed to list models"; "error" => ?err);
            return "[]".to_string();
        }
    };
    let names: Vec<_> = models.models.iter().map(|m| m.name.to_string()).collect();
    serde_json::to_string(&names).unwrap()
}

async fn chat(
    State(_): State<Arc<AppState>>,
    Path(port): Path<u16>,
    Json(request): Json<ChatRequest>,
) -> String {
    let log = DEFAULT.new(o!(
        "function" => "chat",
        "port" => format!("{}", port),
        "model_name" => format!("{}", request.model_name),
    ));
    info!(log, "start");

    let model_name = request.model_name;
    let role = request.role;
    let content = request.content;

    let client = ollama::LLMClient::new(
        ollama::ModelName::new(model_name),
        format!("http://localhost:{}/api", port),
    )
    .unwrap();

    let message = Message {
        role: role.clone(),
        content: content.clone(),
    };

    match client.chat(vec![message]).await {
        Ok(response) => response,
        Err(err) => {
            info!(log, "Failed to chat"; "error" => ?err);
            err.to_string()
        }
    }
}
