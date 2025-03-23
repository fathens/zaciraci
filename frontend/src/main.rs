use yew::prelude::*;
use zaciraci_common::types::{Transaction, TransactionStatus};
use chrono;

#[function_component(App)]
fn app() -> Html {
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

    html! {
        <div class="container">
            <h1>{"Zaciraci Frontend"}</h1>
            <p>{"フロントエンド実装がここに表示されます"}</p>
            
            <h2>{"取引一覧（サンプル）"}</h2>
            <ul>
                {
                    transactions.iter().map(|tx| {
                        let status_text = match tx.status {
                            TransactionStatus::Completed => "完了",
                            TransactionStatus::Pending => "処理中",
                            TransactionStatus::Failed => "失敗",
                        };
                        
                        html! {
                            <li>
                                {format!("ID: {}, 金額: {}, 状態: {}", tx.id, tx.amount, status_text)}
                            </li>
                        }
                    }).collect::<Html>()
                }
            </ul>
        </div>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
