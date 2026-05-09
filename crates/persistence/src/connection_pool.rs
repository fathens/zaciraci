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

/// Test-only connection accessor that asserts the runtime DB is `postgres_test`.
///
/// Test helpers that mutate schema state (e.g. `ALTER TABLE ... DROP CONSTRAINT`
/// inside `insert_data_leakage_violator`) or perform bulk DELETEs against
/// `prediction_records` would silently destroy production data if accidentally
/// pointed at a non-test `DATABASE_URL`. The blast radius is amplified for
/// schema-altering helpers because dropping a `VALIDATED` CHECK constraint and
/// re-adding it as `NOT VALID` is a permanent downgrade of Layer 3 defense.
///
/// This guard is enforced at runtime: `current_database()` must equal
/// `postgres_test`. The check fires on every acquisition (cheap; one round trip).
/// Gated to `cfg(test)` and the `test-helpers` feature so production release
/// binaries (which lack the `test-helpers` feature) do not include it. New test
/// helpers and downstream-crate integration test fixtures that touch DB state
/// MUST acquire connections through this function.
#[cfg(any(test, feature = "test-helpers"))]
pub async fn get_test_only() -> Result<Client> {
    use diesel::RunQueryDsl;
    use diesel::sql_types::Text;
    let conn = POOL.get().await?;

    #[derive(diesel::QueryableByName)]
    struct DbName {
        #[diesel(sql_type = Text)]
        current_database: String,
    }

    let db_name: String = conn
        .interact(|conn| -> diesel::result::QueryResult<String> {
            let row: DbName = diesel::sql_query("SELECT current_database() AS current_database")
                .get_result(conn)?;
            Ok(row.current_database)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Database interaction error: {:?}", e))??;

    const EXPECTED: &str = "postgres_test";
    if db_name != EXPECTED {
        return Err(anyhow::anyhow!(
            "get_test_only() refused: connected to DB '{db_name}', expected '{EXPECTED}'. \
             Test helpers that mutate schema state (e.g. CHECK constraint manipulation) or \
             bulk-delete tables must only run against the test database. Set \
             DATABASE_URL to postgres_test."
        ));
    }
    Ok(conn)
}
