mod connection_pool;
mod token_rate;
pub mod models;
pub mod schema;

use crate::Result;
use diesel::connection::Connection;
use std::sync::Arc;
use anyhow;

/// JSONRPCのnew_client関数と同様に、データベースクライアントのインスタンスを取得します
pub fn new_client() -> Arc<DatabaseClient> {
    Arc::new(DatabaseClient::new())
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