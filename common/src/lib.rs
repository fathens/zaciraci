pub mod config;
pub mod ollama;
pub mod pools;
pub mod stats;
pub mod types;

use serde::{Deserialize, Serialize};

type Result<T> = anyhow::Result<T>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ApiResponse<T, E>
where
    T: std::fmt::Debug + Clone,
    E: std::fmt::Debug + Clone,
    E: std::fmt::Display,
{
    Success(T),
    Error(E),
}
