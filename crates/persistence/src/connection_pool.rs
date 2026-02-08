use crate::Result;
use deadpool_diesel::postgres::Pool;
use deadpool_diesel::{Manager, ManagerConfig, RecyclingMethod};
use once_cell::sync::Lazy;

pub type Client = deadpool_diesel::postgres::Connection;

static POOL: Lazy<Pool> = Lazy::new(|| {
    let max_size: usize = common::config::get("PG_POOL_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let dsn = common::config::get("PG_DSN").unwrap_or_else(|_| {
        "postgres://postgres_test:postgres_test@localhost:5433/postgres_test".to_string()
    });
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(dsn, deadpool_diesel::Runtime::Tokio1, mgr_config);
    Pool::builder(mgr).max_size(max_size).build().unwrap()
});

pub async fn get() -> Result<Client> {
    Ok(POOL.get().await?)
}
