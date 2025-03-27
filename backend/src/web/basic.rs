use axum::Router;
use axum::routing::get;
use crate::jsonrpc;
use crate::wallet;
use crate::types::MicroNear;
use crate::jsonrpc::{AccountInfo, SendTx};
use crate::wallet::Wallet;
use std::sync::Arc;
use axum::extract::{Path, State};
use super::AppState;

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route("/healthcheck", get(|| async { "OK" }))
        .route("/native_token/balance", get(native_token_balance))
        .route(
            "/native_token/transfer/{receiver}/{amount}",
            get(native_token_transfer),
        )
}

async fn native_token_balance(State(_): State<Arc<AppState>>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let account = wallet.account_id();
    let res = client.get_native_amount(account).await;
    match res {
        Ok(balance) => {
            format!("Balance: {balance:?}\n")
        }
        Err(err) => {
            format!("Error: {err}")
        }
    }
}

async fn native_token_transfer(
    State(_): State<Arc<AppState>>,
    Path((receiver, amount)): Path<(String, String)>,
) -> String {
    let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
    let amount = MicroNear::of(amount_micro).to_yocto();
    let receiver = receiver.parse().unwrap();
    let wallet = wallet::new_wallet();
    let signer = wallet.signer();
    let client = jsonrpc::new_client();
    let res = client
        .transfer_native_token(signer, &receiver, amount)
        .await;
    match res {
        Ok(_) => "OK".to_owned(),
        Err(err) => {
            format!("Error: {err}")
        }
    }
}
