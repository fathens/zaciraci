use tokio_postgres::NoTls;

pub use error::Error;
mod error;

type Result<T> = std::result::Result<T, Error>;

pub struct Persistence {
    db: tokio_postgres::Client,
}

impl Persistence {
    pub async fn new() -> Result<Self> {
        let dsn = std::env::var("PG_DSN").unwrap();
        let (client, connection) = tokio_postgres::connect(&dsn, NoTls)
            .await
            .map_err(Error::from)?;

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        Ok(Persistence { db: client })
    }

    pub async fn get_counter(&self) -> Result<u32> {
        let row = self
            .db
            .query_one("SELECT value FROM counter", &[])
            .await
            .map_err(Error::from)?;
        let value: i32 = row.get("value");
        Ok(value.unsigned_abs())
    }

    pub async fn increment(&self) -> Result<u32> {
        let prev = self.get_counter().await?;
        let next = prev + 1;
        let value = next as i32;
        self.db
            .execute("UPDATE counter SET value = $1", &[&value])
            .await
            .map_err(Error::from)?;
        Ok(next)
    }
}
