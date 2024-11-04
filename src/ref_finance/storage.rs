use crate::logging::*;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::Result;
use crate::{jsonrpc, wallet};
use near_sdk::json_types::U128;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageBalanceBounds {
    pub min: U128,
    pub max: Option<U128>,
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
