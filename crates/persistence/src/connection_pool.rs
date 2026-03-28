use std::time::Duration;

use crate::Result;
use deadpool_diesel::postgres::Pool;
use deadpool_diesel::{Manager, ManagerConfig, RecyclingMethod};
use std::sync::LazyLock;

pub type Client = deadpool_diesel::postgres::Connection;

static POOL: LazyLock<Pool> = LazyLock::new(|| {
    let startup = common::config::startup::get();
    let dsn = if startup.database_url.is_empty() {
        "postgres://postgres_test:postgres_test@localhost:5433/postgres_test".to_string()
    } else {
        startup.database_url.clone()
    };
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    };
    let mgr = Manager::from_config(dsn, deadpool_diesel::Runtime::Tokio1, mgr_config);
    Pool::builder(mgr)
        .max_size(startup.pg_pool_size)
        .runtime(deadpool_diesel::Runtime::Tokio1)
        .wait_timeout(Some(Duration::from_secs(30)))
        .build()
        .expect("Failed to build database connection pool")
});

pub async fn get() -> Result<Client> {
    Ok(POOL.get().await?)
}
