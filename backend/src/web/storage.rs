use super::AppState;
use crate::jsonrpc::SentTx;
use crate::ref_finance::token_account::TokenAccount;
use crate::types::MicroNear;
use crate::wallet::Wallet;
use crate::{jsonrpc, ref_finance, wallet};
use axum::{
    Router,
    extract::{Path, State},
    routing::get,
};
use std::sync::Arc;

fn path(sub: &str) -> String {
    format!("/storage/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("deposit_min"), get(storage_deposit_min))
        .route(&path("deposit/{amount}"), get(storage_deposit))
        .route(
            &path("unregister/{token_account}"),
            get(storage_unregister_token),
        )
        .route(&path("amounts/list"), get(deposit_list))
        .route(&path("amounts/wrap/{amount}"), get(wrap_native_token))
        .route(&path("amounts/unwrap/{amount}"), get(unwrap_native_token))
        .route(
            &path("amounts/deposit/{token_account}/{amount}"),
            get(deposit_token),
        )
        .route(
            &path("amounts/withdraw/{token_account}/{amount}"),
            get(withdraw_token),
        )
}

async fn storage_deposit_min(State(_): State<Arc<AppState>>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let bounds = ref_finance::storage::check_bounds(&client).await.unwrap();
    let value = bounds.min.0;
    let res = crate::ref_finance::storage::deposit(&client, &wallet, value, true).await;
    match res {
        Ok(_) => format!("Deposited: {value}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn storage_deposit(State(_): State<Arc<AppState>>, Path(amount): Path<String>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let amount: u128 = amount.replace("_", "").parse().unwrap();
    let res = crate::ref_finance::storage::deposit(&client, &wallet, amount, false).await;
    match res {
        Ok(_) => format!("Deposited: {amount}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn storage_unregister_token(
    State(_): State<Arc<AppState>>,
    Path(token_account): Path<String>,
) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let token: TokenAccount = token_account.parse().unwrap();
    let res = ref_finance::deposit::unregister_tokens(&client, &wallet, &[token]).await;
    match res {
        Ok(_) => format!("Unregistered: {token_account}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn deposit_list(State(_): State<Arc<AppState>>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let account = wallet.account_id();
    let res = ref_finance::deposit::get_deposits(&client, account).await;
    match res {
        Err(e) => format!("Error: {e}"),
        Ok(deposits) => {
            let mut lines = Vec::new();
            for (token, amount) in deposits.iter() {
                let m = MicroNear::from_yocto(amount.0);
                let line = format!("{token} -> {m:?}");
                lines.push(line);
            }
            lines.sort();
            lines.join("\n")
        }
    }
}

async fn deposit_token(
    State(_): State<Arc<AppState>>,
    Path((token_account, amount)): Path<(String, String)>,
) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
    let amount = MicroNear::of(amount_micro).to_yocto();
    let token = token_account.parse().unwrap();
    let res = ref_finance::deposit::deposit(&client, &wallet, &token, amount).await;
    match res {
        Ok(_) => format!("Deposited: {amount}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn withdraw_token(
    State(_): State<Arc<AppState>>,
    Path((token_account, amount)): Path<(String, String)>,
) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
    let amount = MicroNear::of(amount_micro).to_yocto();
    let token = token_account.parse().unwrap();
    let res = ref_finance::deposit::withdraw(&client, &wallet, &token, amount).await;
    match res {
        Ok(_) => format!("Withdrawn: {amount}"),
        Err(e) => format!("Error: {e}"),
    }
}

async fn wrap_native_token(State(_): State<Arc<AppState>>, Path(amount): Path<String>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
    let amount = MicroNear::of(amount_micro).to_yocto();
    let account = wallet.account_id();
    let before = ref_finance::deposit::wnear::balance_of(&client, account)
        .await
        .unwrap();
    let call = async {
        ref_finance::deposit::wnear::wrap(&client, &wallet, amount)
            .await?
            .wait_for_success()
            .await
    };
    match call.await {
        Ok(_) => {
            let after = ref_finance::deposit::wnear::balance_of(&client, account)
                .await
                .unwrap();
            format!("Wrapped: {amount}\n{before}\n{after}")
        }
        Err(err) => format!("Error: {err}"),
    }
}

async fn unwrap_native_token(State(_): State<Arc<AppState>>, Path(amount): Path<String>) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
    let amount = MicroNear::of(amount_micro).to_yocto();
    let account = wallet.account_id();
    let before = ref_finance::deposit::wnear::balance_of(&client, account)
        .await
        .unwrap();
    let call = async {
        ref_finance::deposit::wnear::unwrap(&client, &wallet, amount)
            .await?
            .wait_for_success()
            .await
    };
    match call.await {
        Ok(_) => {
            let after = ref_finance::deposit::wnear::balance_of(&client, account)
                .await
                .unwrap();
            format!("Unwrapped: {amount}\n{before}\n{after}")
        }
        Err(err) => format!("Error: {err}"),
    }
}
