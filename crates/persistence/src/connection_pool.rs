use crate::Result;
use deadpool_diesel::postgres::Pool;
use deadpool_diesel::{Manager, ManagerConfig, RecyclingMethod};
use once_cell::sync::Lazy;

pub type Client = deadpool_diesel::postgres::Connection;

static POOL: Lazy<Pool> = Lazy::new(|| {
    use common::config::ConfigAccess;
    let max_size: usize = common::config::typed().pg_pool_size();
    let dsn = common::config::typed().database_url().unwrap_or_else(|_| {
        "postgres://postgres_test:postgres_test@localhost:5433/postgres_test".to_string()
    });
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    };
    let mgr = Manager::from_config(dsn, deadpool_diesel::Runtime::Tokio1, mgr_config);
    Pool::builder(mgr)
        .max_size(max_size)
        .build()
        .expect("Failed to build database connection pool")
});

pub async fn get() -> Result<Client> {
    Ok(POOL.get().await?)
}
