mod connection_pool;

use crate::Result;

pub struct Persistence {
    pool: deadpool_postgres::Pool,
}

impl Persistence {
    pub async fn new() -> Result<Self> {
        Ok(Persistence {
            pool: connection_pool::get(),
        })
    }

    async fn client(&self) -> Result<deadpool_postgres::Client> {
        Ok(self.pool.get().await?)
    }

    pub async fn get_counter(&self) -> Result<u32> {
        let row = self
            .client()
            .await?
            .query_one("SELECT value FROM counter", &[])
            .await?;
        let value: i32 = row.get("value");
        Ok(value.unsigned_abs())
    }

    pub async fn increment(&self) -> Result<u32> {
        let prev = self.get_counter().await?;
        let next = prev + 1;
        let value = next as i32;
        self.client()
            .await?
            .execute("UPDATE counter SET value = $1", &[&value])
            .await?;
        Ok(next)
    }
}
