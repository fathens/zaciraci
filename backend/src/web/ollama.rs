use super::AppState;
use crate::logging::*;
use crate::ollama;
use axum::{
    Router,
    extract::{Json, Path, State},
    routing::{get, post},
};
use std::sync::Arc;
use zaciraci_common::ollama::{ChatRequest, GenerateRequest};

fn path(sub: &str) -> String {
    format!("/ollama/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("model_names/{port}"), get(list_model_names))
        .route(&path("chat/{port}"), post(chat))
        .route(&path("generate/{port}"), post(generate))
}

fn mk_url(port: u16) -> String {
    format!("http://localhost:{}/api", port)
}

async fn list_model_names(Path(port): Path<u16>) -> String {
    let log = DEFAULT.new(o!(
        "function" => "list_model_names",
        "port" => format!("{}", port)
    ));
    let models = match ollama::list_models(&mk_url(port)).await {
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

    let client = match ollama::LLMClient::new_by_name(request.model_name, mk_url(port)).await {
        Ok(client) => client,
        Err(err) => {
            info!(log, "Failed to create client"; "error" => ?err);
            return err.to_string();
        }
    };

    let res = match client.chat(request.messages).await {
        Ok(response) => response,
        Err(err) => {
            info!(log, "Failed to chat"; "error" => ?err);
            err.to_string()
        }
    };
    serde_json::to_string(&res).unwrap()
}

async fn generate(
    State(_): State<Arc<AppState>>,
    Path(port): Path<u16>,
    Json(request): Json<GenerateRequest>,
) -> String {
    let log = DEFAULT.new(o!(
        "function" => "generate",
        "port" => format!("{}", port),
        "model_name" => format!("{}", request.model_name),
    ));
    info!(log, "start");

    let prompt = request.prompt;
    let images = request.images;

    let client = match ollama::LLMClient::new_by_name(request.model_name, mk_url(port)).await {
        Ok(client) => client,
        Err(err) => {
            info!(log, "Failed to create client"; "error" => ?err);
            return err.to_string();
        }
    };

    let res = match client.generate(prompt, images).await {
        Ok(response) => response,
        Err(err) => {
            info!(log, "Failed to generate"; "error" => ?err);
            err.to_string()
        }
    };
    serde_json::to_string(&res).unwrap()
}
