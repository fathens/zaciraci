mod connection_pool;

use crate::Result;
use diesel::prelude::*;
use once_cell::sync::Lazy;
use std::sync::Arc;

static DB_CLIENT: Lazy<Arc<DatabaseClient>> = Lazy::new(|| {
    Arc::new(DatabaseClient::new())
});

pub struct DatabaseClient {}

impl DatabaseClient {
    fn new() -> Self {
        Self {}
    }
    
    // データベース接続を取得するメソッド
    pub async fn get_connection(&self) -> Result<connection_pool::Client> {
        connection_pool::get().await
    }
    
    // トランザクション内で処理を実行するメソッド
    pub async fn with_transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::PgConnection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.get_connection().await?;
        conn.interact(move |conn| {
            let mut transaction = conn.begin_transaction()?;
            let result = f(&mut transaction)?;
            transaction.commit()?;
            Ok(result)
        })
        .await?
    }
    
    // 単純なデータベース操作を実行するメソッド
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::PgConnection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.get_connection().await?;
        conn.interact(move |conn| f(conn)).await?
    }
}

/// JSONRPCのnew_client関数と同様に、データベースクライアントのインスタンスを取得します
pub fn new_client() -> Arc<DatabaseClient> {
    Arc::clone(&DB_CLIENT)
}

/// すでに存在するget_client関数もそのまま残しておきます
pub fn get_client() -> Arc<DatabaseClient> {
    Arc::clone(&DB_CLIENT)
}