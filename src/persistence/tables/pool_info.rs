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

pub async fn update_all(columns: Vec<PoolInfo>) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "update_all"));
    connection_pool::get()
        .await?
        .interact(move |conn| {
            columns.iter().try_for_each(|column| {
                let n = diesel::update(pool_info.find(id))
                    .set((
                        pool_kind.eq(&column.pool_kind),
                        token_account_id_a.eq(&column.token_account_id_a),
                        token_account_id_b.eq(&column.token_account_id_b),
                        amount_a.eq(&column.amount_a),
                        amount_b.eq(&column.amount_b),
                        total_fee.eq(&column.total_fee),
                        shares_total_supply.eq(&column.shares_total_supply),
                        amp.eq(&column.amp),
                        updated_at.eq(&column.updated_at),
                    ))
                    .returning(PoolInfo::as_returning())
                    .get_result(conn)?;
                trace!(log, "updated"; "n" => n.id);
                Ok(())
            })
        })
        .await?
}
