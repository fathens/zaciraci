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
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = authorized_users)]
struct NewAuthorizedUser {
    pub email: String,
    pub role: String,
}

fn to_role(role_str: &str) -> Result<Role> {
    Role::from_str(role_str).map_err(|_| anyhow!("invalid role value in database"))
}

pub async fn find_by_email(email: &str) -> Result<Option<(String, Role)>> {
    let email = email.to_string();
    let conn = connection_pool::get().await?;

    let result: Option<DbAuthorizedUser> = conn
        .interact(move |conn| {
            authorized_users::table
                .filter(authorized_users::email.eq(&email))
                .select(DbAuthorizedUser::as_select())
                .first(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;

    match result {
        Some(user) => {
            let role = to_role(&user.role)?;
            Ok(Some((user.email, role)))
        }
        None => Ok(None),
    }
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
        .iter()
        .map(|user| {
            let role = to_role(&user.role)?;
            Ok((user.email.clone(), role))
        })
        .collect()
}

pub async fn upsert(email: &str, role: Role) -> Result<()> {
    let new_user = NewAuthorizedUser {
        email: email.to_string(),
        role: role.to_string(),
    };
    let role_str = role.to_string();
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
    let email = email.to_string();
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
