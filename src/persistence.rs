mod connection_pool;

use crate::Result;
use diesel::connection::Connection;
use once_cell::sync::Lazy;
use std::sync::Arc;
use anyhow;

static DB_CLIENT: Lazy<Arc<DatabaseClient>> = Lazy::new(|| {
    Arc::new(DatabaseClient::new())
});

/// JSONRPCのnew_client関数と同様に、データベースクライアントのインスタンスを取得します
pub fn new_client() -> Arc<DatabaseClient> {
    Arc::clone(&DB_CLIENT)
}

#[derive(Clone, Debug)]
pub struct DatabaseClient {}

impl DatabaseClient {
    fn new() -> Self {
        Self {}
    }
    
    // データベース接続を取得するメソッド
    #[allow(dead_code)]
    pub async fn get_connection(&self) -> Result<connection_pool::Client> {
        connection_pool::get().await
    }
    
    // トランザクション内で処理を実行するメソッド
    #[allow(dead_code)]
    pub async fn with_transaction<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::PgConnection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.get_connection().await?;
        match conn.interact(move |conn| {
            Connection::transaction(conn, |tx| {
                // `f`の結果を返して一度Resultを解決
                match f(tx) {
                    Ok(v) => Ok(v),
                    Err(e) => Err(e)
                }
            })
        })
        .await {
            Ok(result) => result,
            Err(_) => Err(anyhow::Error::msg("データベーストランザクションの実行中にエラーが発生しました")),
        }
    }
    
    // 単純なデータベース操作を実行するメソッド
    #[allow(dead_code)]
    pub async fn execute<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut diesel::PgConnection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.get_connection().await?;
        match conn.interact(move |conn| f(conn)).await {
            Ok(result) => result,
            Err(_) => Err(anyhow::Error::msg("データベース操作の実行中にエラーが発生しました")),
        }
    }
}