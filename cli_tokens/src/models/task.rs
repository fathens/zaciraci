use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task_id: String,
    pub created_at: DateTime<Utc>,
    pub token_file: PathBuf,
    pub model: String,
    pub params: PredictionParams,

    // Status tracking
    pub last_status: String,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub poll_count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PredictionParams {
    pub start_pct: f64,
    pub end_pct: f64,
    pub forecast_ratio: f64,
}

impl TaskInfo {
    pub fn new(
        task_id: String,
        token_file: PathBuf,
        model: String,
        params: PredictionParams,
    ) -> Self {
        Self {
            task_id,
            created_at: Utc::now(),
            token_file,
            model,
            params,
            last_status: "pending".to_string(),
            last_checked_at: None,
            poll_count: 0,
        }
    }

    pub fn update_status(&mut self, status: String) {
        self.last_status = status;
        self.last_checked_at = Some(Utc::now());
        self.poll_count += 1;
    }
}
