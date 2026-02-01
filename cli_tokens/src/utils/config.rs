use std::env;

pub struct Config {
    pub backend_url: String,
    pub timeout_seconds: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            backend_url: "http://localhost:8080".to_string(),
            timeout_seconds: 300,
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            backend_url: env::var("BACKEND_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            timeout_seconds: env::var("TIMEOUT_SECONDS")
                .unwrap_or_else(|_| "300".to_string())
                .parse()
                .unwrap_or(300),
        }
    }
}
