use crate::logging::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::{jsonrpc, wallet, Result};
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde_json::json;
use std::collections::HashMap;

pub async fn deposit(token: &TokenAccount, amount: u128) -> Result<()> {
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

pub async fn get_deposits(account: &AccountId) -> Result<HashMap<TokenAccount, U128>> {
    let log = DEFAULT.new(o!(
        "function" => "get_deposits",
        "account" => format!("{}", account),
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "get_deposits";
    let args = json!({
        "account_id": account,
    });

    let result = jsonrpc::view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args).await?;

    let deposits: HashMap<TokenAccount, U128> = serde_json::from_slice(&result.result)?;
    info!(log, "deposits"; "deposits" => ?deposits);
    Ok(deposits)
}

pub async fn unregister_tokens(tokens: &[TokenAccount]) -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "unregister_tokens",
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "unregister_tokens";
    let args = json!({
        "token_ids": tokens
    });

    let deposit = 1; // minimum deposit
    let signer = wallet::WALLET.signer();

    jsonrpc::exec_contract(&signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_deposits() {
        let token = "wrap.testnet".parse().unwrap();
        let account = "app.zaciraci.testnet".parse().unwrap();
        let result = get_deposits(&account).await;
        assert!(result.is_ok());
        let deposits = result.unwrap();
        assert!(!deposits.is_empty());
        assert!(deposits.contains_key(&token));
    }
}
