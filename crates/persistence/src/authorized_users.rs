use crate::Result;
use crate::connection_pool;
use crate::schema::authorized_users;
use anyhow::{Context, anyhow};
use common::types::{Email, Role};
use diesel::prelude::*;
use std::str::FromStr;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = authorized_users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbAuthorizedUser {
    email: String,
    role: String,
}

fn to_role(role_str: &str) -> Result<Role> {
    Role::from_str(role_str)
        .map_err(anyhow::Error::from)
        .with_context(|| format!("invalid role value in database: {role_str}"))
}

fn to_email(raw: &str) -> Result<Email> {
    Email::new(raw)
        .map_err(anyhow::Error::from)
        .with_context(|| "invalid email value in database".to_string())
}

/// Hard upper bound on the number of authorized_users rows that
/// `list_all` will load into memory. The runtime calls this function on
/// startup and on every periodic refresh, so an unbounded `SELECT *`
/// against a runaway-large table would risk an OOM-driven restart loop.
/// The current operator workflow expects O(10) rows, so 10_000 is many
/// orders of magnitude of headroom while still bounding the worst case.
const LIST_ALL_HARD_LIMIT: i64 = 10_000;

pub async fn list_all() -> Result<Vec<(Email, Role)>> {
    let conn = connection_pool::get().await?;

    let results: Vec<DbAuthorizedUser> = conn
        .interact(move |conn| {
            authorized_users::table
                .select(DbAuthorizedUser::as_select())
                .order_by(authorized_users::email.asc())
                .limit(LIST_ALL_HARD_LIMIT + 1)
                .load(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    if results.len() as i64 > LIST_ALL_HARD_LIMIT {
        return Err(anyhow!(
            "authorized_users row count exceeds hard limit ({}); refusing to load to avoid runaway memory use",
            LIST_ALL_HARD_LIMIT
        ));
    }

    results
        .into_iter()
        .map(|user| {
            let email = to_email(&user.email)?;
            let role = to_role(&user.role)?;
            Ok((email, role))
        })
        .collect()
}

// NOTE: Write-path functions (upsert/delete) were intentionally omitted
// from this module. The runtime only needs `list_all()` for UserCache
// bootstrap; authorized user management is expected to be operator-driven
// (direct SQL) until a management RPC lands. Write helpers — along with
// their tests — should be reintroduced as `pub(crate)` at that time and
// must target the `LOWER(email)` functional UNIQUE index via:
//   INSERT ... ON CONFLICT ((lower(email))) DO UPDATE ...
//   DELETE ... WHERE lower(email) = lower($1)
// so that manually-inserted mixed-case rows remain reachable from the
// application layer.

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers;

#[cfg(test)]
mod tests;
