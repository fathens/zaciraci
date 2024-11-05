use crate::logging::*;
use crate::ref_finance::token_account::TokenAccount;
use crate::ref_finance::{deposit, CONTRACT_ADDRESS};
use crate::Result;
use crate::{jsonrpc, wallet};
use near_primitives::types::AccountId;
use near_sdk::json_types::U128;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageBalanceBounds {
    pub min: U128,
    pub max: Option<U128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageBalance {
    pub total: U128,
    pub available: U128,
}

pub async fn check_bounds() -> Result<StorageBalanceBounds> {
    let log = DEFAULT.new(o!("function" => "storage::check_bounds"));
    const METHOD_NAME: &str = "storage_balance_bounds";
    let args = json!({});
    let result = jsonrpc::view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args).await?;

    let bounds: StorageBalanceBounds = serde_json::from_slice(&result.result)?;
    info!(log, "bounds"; "min" => ?bounds.min, "max" => ?bounds.max);
    Ok(bounds)
}

pub async fn deposit(value: u128, registration_only: bool) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "storage::deposit"));
    const METHOD_NAME: &str = "storage_deposit";
    let args = json!({
        "registration_only": registration_only,
    });
    let signer = wallet::WALLET.signer();
    info!(log, "depositing";
        "value" => value,
        "signer" => ?signer.account_id,
    );

    jsonrpc::exec_contract(&signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, value).await?;
    Ok(())
}

pub async fn balance_of(account: AccountId) -> Result<StorageBalance> {
    let log = DEFAULT.new(o!("function" => "storage::balance_of"));
    const METHOD_NAME: &str = "storage_balance_of";
    let args = json!({
        "account_id": account,
    });
    let result = jsonrpc::view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args).await?;

    let balance: StorageBalance = serde_json::from_slice(&result.result)?;
    info!(log, "balance";
        "total" => ?balance.total,
        "available" => ?balance.available,
    );
    Ok(balance)
}

pub async fn check_deposits(account: AccountId, tokens: &[TokenAccount]) -> Result<u128> {
    let log = DEFAULT.new(o!("function" => "storage::check_deposits"));

    let bounds = check_bounds().await?;
    let deposits = deposit::get_deposits(account.clone()).await?;
    let balance = balance_of(account).await?;

    let total = balance.total.0;
    let available = balance.available.0;
    let used = total - available;
    let per_token = (used - bounds.min.0) / deposits.len() as u128;

    info!(log, "checking deposits";
        "total" => total,
        "available" => available,
        "used" => used,
        "per_token" => per_token,
    );

    let mores: Vec<_> = tokens
        .iter()
        .filter(|&token| !deposits.contains_key(token))
        .collect();
    let more_needed = mores.len() as u128 * per_token;
    info!(log, "missing token deposits"; "more_needed" => more_needed);
    if more_needed <= available {
        return Ok(0);
    }

    // TODO: 既存の deposits を削除して足りるかを計算

    Ok(total)
}

pub async fn check_and_deposit(account: AccountId, tokens: &[TokenAccount]) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "storage::check_and_deposit"));

    let more = check_deposits(account.clone(), tokens).await?;
    if more == 0 {
        return Ok(());
    }
    info!(log, "needing more deposit"; "more" => more);
    deposit(more, false).await?;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn test_check_bounds() {
        let res = check_bounds().await;
        assert_eq!(res.clone().err(), None);
        let bounds = res.unwrap();
        assert!(bounds.min >= U128(1_000_000_000_000_000_000_000));
        assert!(bounds.max.unwrap_or(U128(0)) >= U128(0));
    }
}
