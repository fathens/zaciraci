use super::super::schema::pool_info::dsl::*;
use crate::logging::{o, trace, DEFAULT};
use crate::persistence::connection_pool;
use crate::Result;
use bigdecimal::BigDecimal;
use diesel::prelude::*;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::persistence::schema::pool_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PoolInfo {
    pub id: i32,
    pub pool_kind: String,
    pub token_account_ids: Vec<String>,
    pub amounts: Vec<BigDecimal>,
    pub total_fee: i64,
    pub shares_total_supply: BigDecimal,
    pub amp: BigDecimal,
    pub updated_at: chrono::NaiveDateTime,
}

pub async fn select_all() -> Result<Vec<PoolInfo>> {
    let log = DEFAULT.new(o!("function" => "select_all"));
    trace!(log, "start");
    let result = connection_pool::get()
        .await?
        .interact(|conn| pool_info.order_by(id).load::<PoolInfo>(conn))
        .await??;
    trace!(log, "finish"; "count" => result.len());
    Ok(result)
}

pub async fn delete_all() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "delete_all"));
    trace!(log, "start");
    let result = connection_pool::get()
        .await?
        .interact(|conn| diesel::delete(pool_info).execute(conn))
        .await?;
    trace!(log, "finish"; "count" => result?);
    Ok(())
}

pub async fn update_all(records: Vec<PoolInfo>) -> Result<usize> {
    let log = DEFAULT.new(o!(
        "function" => "update_all",
        "count" => records.len(),
    ));
    trace!(log, "start");
    let result = connection_pool::get()
        .await?
        .interact(move |conn| {
            conn.transaction(|conn| {
                records.iter().try_fold(0, |n, record| {
                    diesel::insert_into(pool_info)
                        .values(record)
                        .on_conflict(id)
                        .do_update()
                        .set((
                            pool_kind.eq(&record.pool_kind),
                            token_account_ids.eq(&record.token_account_ids),
                            amounts.eq(&record.amounts),
                            total_fee.eq(&record.total_fee),
                            shares_total_supply.eq(&record.shares_total_supply),
                            amp.eq(&record.amp),
                            updated_at.eq(&record.updated_at),
                        ))
                        .execute(conn)
                        .map(|m| n + m)
                })
            })
        })
        .await?;
    trace!(log, "finish");
    Ok(result?)
}
