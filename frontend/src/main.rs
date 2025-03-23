use chrono;
use dioxus::prelude::*;
use tarpc::{client, context, serde_transport::tcp};
use tokio_serde::formats::Json;
use zaciraci_common::rpc::ZaciraciServiceClient;
use zaciraci_common::types::{Transaction, TransactionStatus};

pub use zaciraci_common::config;
type Result<T> = anyhow::Result<T>;

fn main() {
    // 正しいlaunch関数の呼び出し方法
    dioxus_web::launch::launch_cfg(App, dioxus_web::Config::default());
}

#[component]
fn App() -> Element {
    // サンプルデータ（実際の実装ではAPIから取得）
    let transactions = vec![
        Transaction {
            id: "tx1".to_string(),
            amount: "123.45".to_string(),
            timestamp: chrono::Utc::now(),
            status: TransactionStatus::Completed,
        },
        Transaction {
            id: "tx2".to_string(),
            amount: "67.89".to_string(),
            timestamp: chrono::Utc::now(),
            status: TransactionStatus::Pending,
        },
    ];

    rsx! {
        div { class: "container",
            h1 { "Zaciraci Frontend" }
            p { "フロントエンド実装がここに表示されます" }

            h2 { "取引一覧（サンプル）" }
            ul {
                {transactions.into_iter().map(|tx| {
                    let status_text = match tx.status {
                        TransactionStatus::Completed => "完了",
                        TransactionStatus::Pending => "処理中",
                        TransactionStatus::Failed => "失敗",
                    };

                    rsx! {
                        li {
                            key: "{tx.id}",
                            "ID: {tx.id}, 金額: {tx.amount}, 状態: {status_text}"
                        }
                    }
                })}
            }
        }
    }
}

// クライアント接続用のヘルパー関数
#[allow(dead_code)]
pub async fn connect() -> Result<ZaciraciServiceClient> {
    let host = config::get("ZACIRACI_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let addr = host.parse::<std::net::SocketAddr>()?;
    let transport = tcp::connect(addr, Json::default).await?;
    Ok(ZaciraciServiceClient::new(client::Config::default(), transport).spawn())
}

// クライアント使用例
#[allow(dead_code)]
pub async fn client_example() -> Result<String> {
    let client = connect().await?;
    let ctx = context::current();

    // 健全性チェック
    let health = client.healthcheck(ctx).await?;
    println!("ヘルスチェック結果: {}", health);

    // 残高チェック
    let balance = client.native_token_balance(ctx).await?;
    println!("残高: {}", balance);

    Ok(balance)
}
