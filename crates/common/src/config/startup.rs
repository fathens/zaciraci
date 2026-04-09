use serde::de::DeserializeOwned;
use std::fmt;
use std::str::FromStr;
use std::sync::LazyLock;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct RpcEndpoint {
    pub url: String,
    #[serde(default = "default_weight")]
    pub weight: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_weight() -> u32 {
    10
}

fn default_max_retries() -> u32 {
    3
}

/// Startup-time configuration resolved once from env > defaults.
///
/// These values are fixed after process start and never change.
/// Unlike `ConfigAccess` (which goes through CONFIG_STORE > DB_STORE > env > defaults),
/// `StartupConfig` only reads environment variables with hardcoded defaults.
///
/// `Debug` is implemented manually so secret fields
/// (`database_url`, `root_mnemonic`, `google_client_id`) are replaced
/// with `"[redacted]"` when logged. The raw strings should only be read
/// via the explicit typed accessors — never via `{:?}`.
#[derive(Clone)]
pub struct StartupConfig {
    pub is_mainnet: bool,
    pub database_url: String,
    pub pg_pool_size: usize,
    pub rust_log_format: String,
    pub rpc_endpoints: Vec<RpcEndpoint>,
    pub rpc_failure_reset_seconds: u64,
    pub root_account_id: String,
    pub root_mnemonic: String,
    pub root_hdpath: String,
    pub instance_id: String,
    /// Google OAuth2 client id used as the `aud` claim when verifying
    /// ID tokens. Empty when authentication is not configured; in that
    /// case authenticated endpoints reject every request.
    pub google_client_id: String,
}

impl fmt::Debug for StartupConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        /// Report whether a secret-bearing field is configured without
        /// revealing its value. Empty → `"[unset]"` so a misconfiguration
        /// is still visible in logs; non-empty → `"[redacted]"`.
        fn presence(s: &str) -> &'static str {
            if s.is_empty() {
                "[unset]"
            } else {
                "[redacted]"
            }
        }
        f.debug_struct("StartupConfig")
            .field("is_mainnet", &self.is_mainnet)
            .field("database_url", &presence(&self.database_url))
            .field("pg_pool_size", &self.pg_pool_size)
            .field("rust_log_format", &self.rust_log_format)
            .field("rpc_endpoints", &self.rpc_endpoints)
            .field("rpc_failure_reset_seconds", &self.rpc_failure_reset_seconds)
            .field("root_account_id", &self.root_account_id)
            .field("root_mnemonic", &presence(&self.root_mnemonic))
            .field("root_hdpath", &self.root_hdpath)
            .field("instance_id", &self.instance_id)
            .field("google_client_id", &presence(&self.google_client_id))
            .finish()
    }
}

impl StartupConfig {
    /// Resolve from env > defaults. Always succeeds (required values validated by consumers).
    fn resolve() -> Self {
        Self {
            is_mainnet: env_parse("USE_MAINNET").unwrap_or(true),
            database_url: env_string("DATABASE_URL").unwrap_or_default(),
            pg_pool_size: env_parse("PG_POOL_SIZE").unwrap_or(2),
            rust_log_format: env_string("RUST_LOG_FORMAT").unwrap_or_else(|| "json".to_string()),
            rpc_endpoints: env_json("RPC_ENDPOINTS").unwrap_or_default(),
            rpc_failure_reset_seconds: env_parse("RPC_FAILURE_RESET_SECONDS").unwrap_or(300),
            root_account_id: env_string("ROOT_ACCOUNT_ID").unwrap_or_default(),
            root_mnemonic: env_string("ROOT_MNEMONIC").unwrap_or_default(),
            root_hdpath: env_string("ROOT_HDPATH").unwrap_or_else(|| "m/44'/397'/0'".to_string()),
            instance_id: env_string("INSTANCE_ID").unwrap_or_else(|| "*".to_string()),
            google_client_id: env_string("GOOGLE_CLIENT_ID").unwrap_or_default(),
        }
    }
}

static STARTUP: LazyLock<StartupConfig> = LazyLock::new(StartupConfig::resolve);

/// Returns a reference to the global startup configuration.
///
/// Resolved on first access from env > defaults.
pub fn get() -> &'static StartupConfig {
    &STARTUP
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn env_parse<T: FromStr>(key: &str) -> Option<T> {
    env_string(key).and_then(|v| v.parse().ok())
}

fn env_json<T: DeserializeOwned>(key: &str) -> Option<T> {
    env_string(key).and_then(|v| serde_json::from_str(&v).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_string_returns_none_for_missing() {
        assert!(env_string("ZACIRACI_TEST_NONEXISTENT_KEY_XYZ").is_none());
    }

    #[test]
    fn test_env_parse_returns_none_for_missing() {
        assert!(env_parse::<u64>("ZACIRACI_TEST_NONEXISTENT_KEY_XYZ").is_none());
    }

    #[test]
    fn test_startup_config_resolve_succeeds() {
        // resolve() should always succeed even without env vars
        let config = StartupConfig::resolve();
        assert!(config.is_mainnet);
        assert_eq!(config.pg_pool_size, 2);
        assert_eq!(config.rust_log_format, "json");
        assert_eq!(config.rpc_failure_reset_seconds, 300);
        assert_eq!(config.root_hdpath, "m/44'/397'/0'");
        assert_eq!(config.instance_id, "*");
    }

    #[test]
    fn test_get_returns_static_ref() {
        let s1 = get();
        let s2 = get();
        assert!(std::ptr::eq(s1, s2));
    }

    #[test]
    fn debug_redacts_secret_fields() {
        let config = StartupConfig {
            is_mainnet: true,
            database_url: "postgres://user:supersecret@host/db".to_string(),
            pg_pool_size: 4,
            rust_log_format: "json".to_string(),
            rpc_endpoints: vec![],
            rpc_failure_reset_seconds: 60,
            root_account_id: "alice.near".to_string(),
            root_mnemonic: "word1 word2 word3 ... word12".to_string(),
            root_hdpath: "m/44'/397'/0'".to_string(),
            instance_id: "primary".to_string(),
            google_client_id: "123-abc.apps.googleusercontent.com".to_string(),
        };
        let formatted = format!("{config:?}");
        assert!(!formatted.contains("supersecret"));
        assert!(!formatted.contains("word1 word2 word3"));
        assert!(!formatted.contains("123-abc.apps.googleusercontent.com"));
        assert!(formatted.contains("[redacted]"));
        // Non-secret fields remain readable.
        assert!(formatted.contains("alice.near"));
        assert!(formatted.contains("primary"));
    }

    #[test]
    fn debug_shows_unset_for_empty_secret_fields() {
        let config = StartupConfig {
            is_mainnet: false,
            database_url: String::new(),
            pg_pool_size: 1,
            rust_log_format: "plain".to_string(),
            rpc_endpoints: vec![],
            rpc_failure_reset_seconds: 0,
            root_account_id: String::new(),
            root_mnemonic: String::new(),
            root_hdpath: String::new(),
            instance_id: "*".to_string(),
            google_client_id: String::new(),
        };
        let formatted = format!("{config:?}");
        assert!(formatted.contains("[unset]"));
        assert!(!formatted.contains("[redacted]"));
    }
}
