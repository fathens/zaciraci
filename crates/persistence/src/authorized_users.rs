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

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = authorized_users)]
struct NewAuthorizedUser {
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

pub async fn list_all() -> Result<Vec<(Email, Role)>> {
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
            let email = to_email(&user.email)?;
            let role = to_role(&user.role)?;
            Ok((email, role))
        })
        .collect()
}

pub async fn upsert(email: &Email, role: Role) -> Result<()> {
    let role_str = role.to_string();
    let new_user = NewAuthorizedUser {
        email: email.as_str().to_string(),
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

pub async fn delete(email: &Email) -> Result<()> {
    let email = email.as_str().to_string();
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
