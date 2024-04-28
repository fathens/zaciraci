use super::super::schema::pool_info::dsl::*;
use crate::logging::{o, trace, DEFAULT};
use crate::persistence::connection_pool;
use crate::Result;
use bigdecimal::BigDecimal;
use diesel::prelude::*;

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::persistence::schema::pool_info)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct PoolInfo {
    pub id: i32,
    pub pool_kind: String,
    pub token_account_id_a: String,
    pub token_account_id_b: String,
    pub amount_a: BigDecimal,
    pub amount_b: BigDecimal,
    pub total_fee: i64,
    pub shares_total_supply: BigDecimal,
    pub amp: BigDecimal,
    pub updated_at: chrono::NaiveDateTime,
}

impl PoolInfo {
    pub fn key(&self) -> (String, String, String) {
        (
            self.pool_kind.clone(),
            self.token_account_id_a.clone(),
            self.token_account_id_b.clone(),
        )
    }
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
                            token_account_id_a.eq(&record.token_account_id_a),
                            token_account_id_b.eq(&record.token_account_id_b),
                            amount_a.eq(&record.amount_a),
                            amount_b.eq(&record.amount_b),
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
