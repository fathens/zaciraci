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
            .query_opt("SELECT value FROM counter", &[])
            .await?;
        let value: i32 = row.map(|row| row.get("value")).unwrap_or(0);
        Ok(value.unsigned_abs())
    }

    pub async fn increment(&self) -> Result<u32> {
        let row = self
            .client()
            .await?
            .query_opt("SELECT value FROM counter", &[])
            .await?;
        let prev: Option<i32> = row.map(|row| row.get("value"));
        let next = prev.unwrap_or(0).unsigned_abs() + 1;
        let value = next as i32;
        if prev.is_some() {
            self.client()
                .await?
                .execute("UPDATE counter SET value = $1", &[&value])
                .await?;
        } else {
            self.client()
                .await?
                .execute("INSERT INTO counter (value) VALUES ($1)", &[&value])
                .await?;
        }
        Ok(next)
    }
}
