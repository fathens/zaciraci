use super::AppState;
use crate::jsonrpc;
use crate::jsonrpc::{AccountInfo, SendTx};
use crate::types::MicroNear;
use crate::wallet;
use crate::wallet::Wallet;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
};
use near_sdk::NearToken;
use std::sync::Arc;

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route("/healthcheck", get(|| async { "OK" }))
        .route("/native_token/balance", get(native_token_balance))
        .route(
            "/native_token/transfer/{receiver}/{amount}",
            get(native_token_transfer),
        )
        .route("/token/{token_id}/decimals", get(token_decimals))
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
    let amount_yocto = MicroNear::of(amount_micro).to_yocto();
    let receiver = receiver.parse().unwrap();
    let wallet = wallet::new_wallet();
    let signer = wallet.signer();
    let client = jsonrpc::new_client();
    let res = client
        .transfer_native_token(signer, &receiver, NearToken::from_yoctonear(amount_yocto))
        .await;
    match res {
        Ok(_) => "OK".to_owned(),
        Err(err) => {
            format!("Error: {err}")
        }
    }
}

async fn token_decimals(
    State(_): State<Arc<AppState>>,
    Path(token_id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    let client = jsonrpc::new_client();
    let decimals = crate::trade::market_data::get_token_decimals(&client, &token_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get decimals: {e}"),
            )
        })?;
    Ok(decimals.to_string())
}
