//! Raw fixture helpers for tests that need to seed the
//! `authorized_users` table without going through (and validating
//! against) the application-level `Email` newtype. These are exposed
//! under the `test-helpers` feature so other crates can drive their
//! own integration tests against this table without taking a direct
//! dependency on diesel or the persistence schema.
//!
//! Production code must not call these functions; the gating exists
//! purely so that the helpers do not bloat release builds.

use crate::Result;
use crate::connection_pool;
use anyhow::anyhow;
use diesel::RunQueryDsl;
use diesel::sql_query;
use diesel::sql_types::Text;

/// Insert (or upsert under the case-insensitive index) a single row.
pub async fn raw_upsert(raw_email: &str, role: &str) -> Result<()> {
    let raw_email = raw_email.to_string();
    let role = role.to_string();
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        sql_query(
            "INSERT INTO authorized_users (email, role) VALUES ($1, $2) \
             ON CONFLICT ((lower(email))) DO UPDATE SET email = EXCLUDED.email, role = EXCLUDED.role",
        )
        .bind::<Text, _>(raw_email)
        .bind::<Text, _>(role)
        .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("database interaction error: {:?}", e))??;
    Ok(())
}

/// Delete a single row by case-insensitive email match.
pub async fn raw_delete(raw_email: &str) -> Result<()> {
    let raw_email = raw_email.to_string();
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        sql_query("DELETE FROM authorized_users WHERE lower(email) = lower($1)")
            .bind::<Text, _>(raw_email)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("database interaction error: {:?}", e))??;
    Ok(())
}

/// Delete every row whose email matches the given LIKE pattern. Used
/// to wipe leftover fixture rows from prior test runs at the top of a
/// test.
pub async fn wipe_by_email_like(pattern: &str) -> Result<()> {
    let pattern = pattern.to_string();
    let conn = connection_pool::get().await?;
    conn.interact(move |conn| {
        sql_query("DELETE FROM authorized_users WHERE email LIKE $1")
            .bind::<Text, _>(pattern)
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("database interaction error: {:?}", e))??;
    Ok(())
}
