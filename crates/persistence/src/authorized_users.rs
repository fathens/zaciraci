use crate::Result;
use crate::connection_pool;
use crate::schema::authorized_users;
use anyhow::anyhow;
use common::types::Role;
use diesel::prelude::*;
use std::str::FromStr;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = authorized_users)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbAuthorizedUser {
    email: String,
    role: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = authorized_users)]
struct NewAuthorizedUser {
    email: String,
    role: String,
}

fn to_role(role_str: &str) -> Result<Role> {
    Role::from_str(role_str).map_err(|e| anyhow!("invalid role value in database: {e}"))
}

/// Normalize an email before writing to or querying the DB.
///
/// Google email local parts are effectively case-insensitive, so we store
/// and compare the lowercase/trimmed form to avoid lockouts when admins
/// register a user with mixed-case input.
///
/// Shared with `google_auth::user_cache` so the in-memory lookup key and
/// the DB key cannot drift apart — a drift would silently turn authorized
/// users into `UserNotRegistered` or, worse, match the wrong principal.
pub fn normalize_email(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

pub async fn list_all() -> Result<Vec<(String, Role)>> {
    let conn = connection_pool::get().await?;

    let results: Vec<DbAuthorizedUser> = conn
        .interact(move |conn| {
            authorized_users::table
                .select(DbAuthorizedUser::as_select())
                .order_by(authorized_users::email.asc())
                .load(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    results
        .into_iter()
        .map(|user| {
            let role = to_role(&user.role)?;
            Ok((user.email, role))
        })
        .collect()
}

pub async fn upsert(email: &str, role: Role) -> Result<()> {
    let role_str = role.to_string();
    let new_user = NewAuthorizedUser {
        email: normalize_email(email),
        role: role_str.clone(),
    };
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| {
        diesel::insert_into(authorized_users::table)
            .values(&new_user)
            .on_conflict(authorized_users::email)
            .do_update()
            .set(authorized_users::role.eq(&role_str))
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

pub async fn delete(email: &str) -> Result<()> {
    let email = normalize_email(email);
    let conn = connection_pool::get().await?;

    conn.interact(move |conn| {
        diesel::delete(authorized_users::table.filter(authorized_users::email.eq(&email)))
            .execute(conn)
    })
    .await
    .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    Ok(())
}

#[cfg(test)]
mod tests;
