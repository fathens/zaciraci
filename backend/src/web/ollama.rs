use super::AppState;
use crate::logging::*;
use crate::ollama;
use crate::ollama::get_base_url;
use axum::{
    Router,
    extract::{Json, State},
    routing::{get, post},
};
use std::sync::Arc;
use zaciraci_common::ollama::{ChatRequest, GenerateRequest};

fn path(sub: &str) -> String {
    format!("/ollama/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("model_names"), get(list_model_names))
        .route(&path("chat"), post(chat))
        .route(&path("generate"), post(generate))
}

async fn list_model_names() -> String {
    let log = DEFAULT.new(o!(
        "function" => "list_model_names"
    ));
    let models = match ollama::list_models(&get_base_url()).await {
        Ok(models) => models,
        Err(err) => {
            warn!(log, "Failed to list models"; "error" => ?err);
            return "[]".to_string();
        }
    };
    let names: Vec<_> = models.models.iter().map(|m| m.name.to_string()).collect();
    serde_json::to_string(&names).unwrap()
}

async fn chat(State(_): State<Arc<AppState>>, Json(request): Json<ChatRequest>) -> String {
    let log = DEFAULT.new(o!(
        "function" => "chat",
        "model_name" => format!("{}", request.model_name),
    ));
    warn!(log, "start");

    let client = match ollama::Client::new_by_name(&request.model_name, get_base_url()).await {
        Ok(client) => client,
        Err(err) => {
            warn!(log, "Failed to create client"; "error" => ?err);
            return err.to_string();
        }
    };

    let res = match client.chat(request.messages).await {
        Ok(response) => response,
        Err(err) => {
            warn!(log, "Failed to chat"; "error" => ?err);
            err.to_string()
        }
    };
    serde_json::to_string(&res).unwrap()
}

async fn generate(State(_): State<Arc<AppState>>, Json(request): Json<GenerateRequest>) -> String {
    let log = DEFAULT.new(o!(
        "function" => "generate",
        "model_name" => format!("{}", request.model_name),
    ));
    warn!(log, "start");

    let prompt = request.prompt;
    let images = request.images;

    let client = match ollama::Client::new_by_name(&request.model_name, get_base_url()).await {
        Ok(client) => client,
        Err(err) => {
            warn!(log, "Failed to create client"; "error" => ?err);
            return err.to_string();
        }
    };

    let res = match client.generate(prompt, images).await {
        Ok(response) => response,
        Err(err) => {
            warn!(log, "Failed to generate"; "error" => ?err);
            err.to_string()
        }
    };
    serde_json::to_string(&res).unwrap()
}
