mod connection_pool;
mod schema;
pub mod tables;

use crate::logging::*;
use crate::Result;
use diesel::prelude::*;
use schema::counter::dsl::*;

pub struct Persistence {}

impl Persistence {
    pub async fn new() -> Result<Self> {
        Ok(Persistence {})
    }

    async fn get_counter_opt(&self) -> Result<Option<i32>> {
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

    pub async fn get_counter(&self) -> Result<u32> {
        let maybe_value = self.get_counter_opt().await?;
        Ok(maybe_value.unwrap_or(0).unsigned_abs())
    }

    pub async fn increment(&self) -> Result<u32> {
        let log = DEFAULT.new(o!("function" => "increment"));

        let prev: Option<i32> = self.get_counter_opt().await?;
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
}
