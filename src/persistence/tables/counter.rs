use crate::logging::DEFAULT;
use crate::persistence::schema::counter::dsl::counter;
use crate::persistence::schema::counter::value;
use crate::persistence::{connection_pool, tables};
use diesel::prelude::*;
use slog::{o, trace};

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::persistence::schema::counter)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Counter {
    pub value: i32,
}

async fn get_counter_opt() -> crate::Result<Option<i32>> {
    let row = connection_pool::get()
        .await?
        .interact(|conn| {
            counter
                .select(tables::counter::Counter::as_select())
                .first(conn)
        })
        .await?;
    Ok(row.ok().map(|row| row.value))
}

pub async fn get() -> crate::Result<u32> {
    let maybe_value = get_counter_opt().await?;
    Ok(maybe_value.unwrap_or(0).unsigned_abs())
}

pub async fn increment() -> crate::Result<u32> {
    let log = DEFAULT.new(o!("function" => "increment"));

    let prev: Option<i32> = get_counter_opt().await?;
    let next = prev.unwrap_or(0).unsigned_abs() + 1;
    let next_value = next as i32;
    let ok = if prev.is_some() {
        connection_pool::get()
            .await?
            .interact(move |conn| {
                diesel::update(counter)
                    .set(value.eq(next_value))
                    .execute(conn)
            })
            .await?
    } else {
        connection_pool::get()
            .await?
            .interact(move |conn| {
                let row = tables::counter::Counter { value: next_value };
                diesel::insert_into(counter).values(&row).execute(conn)
            })
            .await?
    }?;
    trace!(log, "incremented counter"; "prev" => prev, "next" => next, "ok" => ok);
    Ok(next)
}
