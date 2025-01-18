use crate::logging::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::{token_account, CONTRACT_ADDRESS};
use crate::{jsonrpc, wallet, Result};
use near_primitives::types::Balance;
use near_sdk::json_types::U128;
use near_sdk::AccountId;
use serde_json::json;
use std::collections::HashMap;

pub async fn wrap_near(amount: Balance) -> Result<TokenAccount> {
    let log = DEFAULT.new(o!(
        "function" => "wrap_near",
        "amount" => amount,
    ));
    info!(log, "wrapping native token");

    const METHOD_NAME: &str = "near_deposit";

    let token = token_account::START_TOKEN.clone();
    let args = json!({});
    let signer = wallet::WALLET.signer();

    jsonrpc::exec_contract(&signer, token.as_id(), METHOD_NAME, &args, amount).await?;
    Ok(token)
}

pub async fn deposit(token: &TokenAccount, amount: Balance) -> Result<()> {
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

pub async fn withdraw(token: &TokenAccount, amount: Balance) -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "withdraw",
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "withdraw";
    let args = json!({
        "token_id": token,
        "amount": U128(amount),
        "skip_unwrap_near": false,
    });

    let deposit = 1; // minimum deposit
    let signer = wallet::WALLET.signer();

    jsonrpc::exec_contract(&signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit).await?;
    Ok(())
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
