use crate::batch;
use crate::connection_pool;
use crate::schema::trade_transactions;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use common::types::TokenSmallestUnits;
use diesel::prelude::*;
use logging::*;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;

#[derive(Debug, Clone, Serialize, Deserialize, Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = trade_transactions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct TradeTransaction {
    pub tx_id: String,
    pub trade_batch_id: String,
    pub from_token: String,
    #[diesel(deserialize_as = BigDecimal)]
    #[diesel(serialize_as = BigDecimal)]
    pub from_amount: TokenSmallestUnits,
    pub to_token: String,
    #[diesel(deserialize_as = BigDecimal)]
    #[diesel(serialize_as = BigDecimal)]
    pub to_amount: TokenSmallestUnits,
    pub timestamp: NaiveDateTime,
    pub evaluation_period_id: String,
    // Nullable カラムでは deserialize_as/serialize_as が Option と互換しないため
    // BigDecimal を直接使用。呼び出し側で TokenSmallestUnits との変換を行う。
    pub actual_to_amount: Option<BigDecimal>,
}

impl TradeTransaction {
    /// Bind-parameter count per row for the chunked batch insert. SSoT for
    /// `chunk_rows` budgeting (`crate::batch`); the structural test in
    /// `tests::cols_matches_struct_fields` enforces field-count alignment.
    const COLS: NonZeroUsize = NonZeroUsize::new(9).expect("COLS must be non-zero");

    pub fn insert(self, conn: &mut PgConnection) -> QueryResult<TradeTransaction> {
        diesel::insert_into(trade_transactions::table)
            .values(self)
            .get_result(conn)
    }

    pub async fn insert_async(self) -> Result<TradeTransaction> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| self.insert(conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to insert trade transaction")
    }

    pub fn insert_batch(
        mut transactions: Vec<Self>,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<TradeTransaction>> {
        // Chunk size derived from `Self::COLS` to stay under the PostgreSQL
        // 65535 bind-parameter limit. See `crate::batch`.
        const CHUNK_ROWS: NonZeroUsize = batch::chunk_rows(TradeTransaction::COLS);

        let total = transactions.len();
        if total > CHUNK_ROWS.get() {
            let log = DEFAULT.new(o!("function" => "TradeTransaction::insert_batch"));
            debug!(log, "batch chunked";
                "rows" => total,
                "chunk_rows" => CHUNK_ROWS.get(),
            );
        }

        conn.transaction(|conn| {
            let mut inserted = Vec::with_capacity(total);
            // `TradeTransaction` doubles as the read model
            // (`Queryable`/`Selectable`) and the insert model (`Insertable`),
            // and the `#[diesel(serialize_as = BigDecimal)]` attributes on
            // the amount fields force diesel to require owned values — there
            // is no `Insertable for &TradeTransaction` derive available, so
            // `.values(chunk)` with a slice (as the other three batch_insert
            // paths do) does not compile. Move chunks out of `transactions`
            // via `Vec::drain` instead of cloning: the heap fields
            // (`String`s, `BigDecimal`s, `Option<BigDecimal>`) are
            // transferred by move per element, avoiding the ~7 per-row
            // allocations the previous `chunk.to_vec()` paid. The owned
            // `Vec<Self>` argument is reused as the working buffer.
            // Splitting a dedicated `NewDbTradeTransaction` insert model
            // would let this drop down to a slice-by-reference call and is
            // left for a future PR.
            while !transactions.is_empty() {
                let take = transactions.len().min(CHUNK_ROWS.get());
                let chunk: Vec<TradeTransaction> = transactions.drain(..take).collect();
                let rows: Vec<TradeTransaction> = diesel::insert_into(trade_transactions::table)
                    .values(chunk)
                    .get_results(conn)?;
                inserted.extend(rows);
            }
            Ok(inserted)
        })
    }

    pub async fn insert_batch_async(transactions: Vec<Self>) -> Result<Vec<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::insert_batch(transactions, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to insert batch of trade transactions")
    }

    pub fn find_by_batch_id(
        batch_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<TradeTransaction>> {
        trade_transactions::table
            .filter(trade_transactions::trade_batch_id.eq(batch_id))
            .order(trade_transactions::timestamp.asc())
            .get_results(conn)
    }

    pub async fn find_by_batch_id_async(batch_id: String) -> Result<Vec<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::find_by_batch_id(&batch_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to find transactions by batch ID")
    }

    pub fn find_by_tx_id(
        tx_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<Option<TradeTransaction>> {
        trade_transactions::table
            .filter(trade_transactions::tx_id.eq(tx_id))
            .first(conn)
            .optional()
    }

    pub async fn find_by_tx_id_async(tx_id: String) -> Result<Option<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::find_by_tx_id(&tx_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to find transaction by tx ID")
    }

    pub fn get_latest_batch_id(conn: &mut PgConnection) -> QueryResult<Option<String>> {
        trade_transactions::table
            .select(trade_transactions::trade_batch_id)
            .order(trade_transactions::timestamp.desc())
            .first::<String>(conn)
            .optional()
    }

    pub async fn get_latest_batch_id_async() -> Result<Option<String>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(Self::get_latest_batch_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to get latest batch ID")
    }

    pub async fn delete_by_tx_id_async(tx_id: String) -> Result<()> {
        let conn = connection_pool::get().await?;

        conn.interact(move |conn| {
            diesel::delete(trade_transactions::table.filter(trade_transactions::tx_id.eq(&tx_id)))
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?
        .map_err(|e| anyhow::anyhow!("Failed to delete transaction: {}", e))?;

        Ok(())
    }

    /// 指定した評価期間のトランザクション数を取得
    pub fn count_by_evaluation_period(
        period_id: &str,
        conn: &mut PgConnection,
    ) -> QueryResult<i64> {
        use diesel::dsl::count;

        trade_transactions::table
            .filter(trade_transactions::evaluation_period_id.eq(period_id))
            .select(count(trade_transactions::tx_id))
            .first(conn)
    }

    pub async fn count_by_evaluation_period_async(period_id: String) -> Result<i64> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::count_by_evaluation_period(&period_id, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to count transactions by evaluation period")
    }

    /// 指定期間内の全取引を取得
    pub fn find_by_date_range(
        start: NaiveDateTime,
        end: NaiveDateTime,
        conn: &mut PgConnection,
    ) -> QueryResult<Vec<TradeTransaction>> {
        trade_transactions::table
            .filter(trade_transactions::timestamp.ge(start))
            .filter(trade_transactions::timestamp.le(end))
            .order(trade_transactions::timestamp.asc())
            .get_results(conn)
    }

    /// 指定期間内の全取引を取得（非同期版）
    pub async fn find_by_date_range_async(
        start: NaiveDateTime,
        end: NaiveDateTime,
    ) -> Result<Vec<TradeTransaction>> {
        let conn = connection_pool::get().await?;

        let result = conn
            .interact(move |conn| Self::find_by_date_range(start, end, conn))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to interact with database: {}", e))?;

        result.context("Failed to find transactions by date range")
    }
}

#[cfg(test)]
mod tests;
