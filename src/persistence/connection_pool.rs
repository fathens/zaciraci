use crate::config;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use once_cell::sync::Lazy;
use tokio_postgres::NoTls;

static POOL: Lazy<Pool> = Lazy::new(|| {
    let max_size: usize = config::get("PG_POOL_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(16);
    let dsn = config::get("PG_DSN").unwrap();
    let pg_config = dsn.parse().unwrap();
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
    Pool::builder(mgr).max_size(max_size).build().unwrap()
});

pub fn get() -> Pool {
    POOL.clone()
}
