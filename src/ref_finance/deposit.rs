use crate::logging::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::{jsonrpc, wallet, Result};
use near_sdk::json_types::U128;
use serde_json::json;

pub async fn deposit(token: TokenAccount, amount: u128) -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "deposit",
        "token" => format!("{}", token),
        "amount" => amount,
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "ft_transfer_call";

    let args = json!({
        "receiver_id": CONTRACT_ADDRESS.clone(),
        "amount": U128(amount),
        "msg": "",
    });

    let deposit = 1; // minimum deposit
    let signer = wallet::WALLET.signer();

    jsonrpc::exec_contract(&signer, token.as_id(), METHOD_NAME, &args, deposit).await?;
    Ok(())
}
