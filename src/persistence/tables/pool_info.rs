use crate::logging::*;
use crate::persistence::connection_pool;
use crate::Result;
use postgres_types::ToSql;

type Column = (dyn ToSql + Sync);

pub async fn update_all(list: Vec<(u16, serde_json::Value)>) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "update_all"));
    info!(log, "start");

    let mut client = connection_pool::get_client().await?;
    let transaction = client.transaction().await?;

    let deleted = transaction.execute("DELETE FROM pool_info", &[]).await?;
    debug!(log, "Deleted from pool_info"; "count" => deleted);

    const BATCH_SIZE: usize = 1;
    for chunk in list.chunks(BATCH_SIZE) {
        let count = chunk.len();
        let query = format!(
            "INSERT INTO pool_info (pool_id, body) VALUES {}",
            (0..count).map(|_| "(?, ?)").collect::<Vec<_>>().join(", ")
        );
        let all_columns: Vec<(i32, String)> = chunk
            .iter()
            .map(|(index, jv)| (*index as i32, jv.to_string()))
            .collect();
        trace!(log, "Inserting into pool_info";
            "count" => count,
            "query" => &query,
            "values" => format!("{:?}", all_columns)
        );
        let mut values: Vec<&Column> = vec![];
        for (index, body) in all_columns.iter() {
            values.push(index);
            values.push(body);
        }
        let inserted = transaction.execute(&query, &values).await?;
        debug!(log, "Inserted into pool_info"; "count" => inserted);
    }

    info!(log, "finish"; "count" => list.len());
    Ok(transaction.commit().await?)
}
