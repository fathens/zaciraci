use crate::logging::*;
use crate::persistence::connection_pool;
use crate::Result;
use diesel::Connection;

pub async fn update_all(list: Vec<(u16, serde_json::Value)>) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "update_all"));
    info!(log, "start");

    connection_pool::get()
        .await?
        .interact(|conn| {
            conn.transaction(|_| {
                for (_id, _value) in list {
                    todo!("update pool_info")
                }
                Ok(())
            })
        })
        .await?
}
