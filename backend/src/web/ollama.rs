use super::AppState;
use crate::logging::*;
use crate::ollama::{self, Image, Message};
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

#[derive(Deserialize)]
pub struct GenerateRequest {
    model_name: String,
    prompt: String,
    images: Vec<Image>,
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route("/model_names", get(list_model_names))
        .route("/chat/{port}", post(chat))
        .route("/generate/{port}", post(generate))
}

fn mk_url(port: u16) -> String {
    format!("http://localhost:{}/api", port)
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

    let model_name = match ollama::find_model(request.model_name).await {
        Ok(model_name) => model_name,
        Err(err) => {
            info!(log, "Failed to find model"; "error" => ?err);
            return err.to_string();
        }
    };
    let client = ollama::LLMClient::new(
        model_name,
        mk_url(port),
    )
    .unwrap();

    let message = Message {
        role: request.role,
        content: request.content,
    };

    match client.chat(vec![message]).await {
        Ok(response) => response,
        Err(err) => {
            info!(log, "Failed to chat"; "error" => ?err);
            err.to_string()
        }
    }
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

    let model_name = match ollama::find_model(request.model_name).await {
        Ok(model_name) => model_name,
        Err(err) => {
            info!(log, "Failed to find model"; "error" => ?err);
            return err.to_string();
        }
    };
    let prompt = request.prompt;
    let images = request.images;

    let client = ollama::LLMClient::new(
        model_name,
        mk_url(port),
    )
    .unwrap();

    match client.generate(prompt, images).await {
        Ok(response) => response,
        Err(err) => {
            info!(log, "Failed to generate"; "error" => ?err);
            err.to_string()
        }
    }
}