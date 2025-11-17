use crate::Result;
use crate::jsonrpc::{SendTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::ref_finance::token_account::TokenAccount;
use crate::wallet::Wallet;
use near_primitives::types::Balance;
use near_sdk::AccountId;
use near_sdk::json_types::U128;
use serde_json::json;
use std::collections::HashMap;

pub mod wnear {
    use crate::Result;
    use crate::jsonrpc::{SendTx, ViewContract};
    use crate::logging::*;
    use crate::ref_finance::token_account::WNEAR_TOKEN;
    use crate::wallet::Wallet;
    use near_primitives::types::Balance;
    use near_sdk::AccountId;
    use near_sdk::json_types::U128;
    use serde_json::json;

    pub async fn balance_of<C: ViewContract>(client: &C, account: &AccountId) -> Result<Balance> {
        let log = DEFAULT.new(o!(
            "function" => "balance_of",
            "account" => format!("{}", account),
        ));
        info!(log, "entered");

        const METHOD_NAME: &str = "ft_balance_of";
        let args = json!({
            "account_id": account,
        });

        let result = client
            .view_contract(WNEAR_TOKEN.as_id(), METHOD_NAME, &args)
            .await?;
        let balance: U128 = serde_json::from_slice(&result.result)?;
        Ok(balance.into())
    }

    pub async fn wrap<C: SendTx, W: Wallet>(
        client: &C,
        wallet: &W,
        amount: Balance,
    ) -> Result<C::Output> {
        let log = DEFAULT.new(o!(
            "function" => "wrap_near",
            "amount" => amount,
        ));
        info!(log, "wrapping native token");

        const METHOD_NAME: &str = "near_deposit";

        let token = WNEAR_TOKEN.clone();
        let args = json!({});
        let signer = wallet.signer();

        client
            .exec_contract(signer, token.as_id(), METHOD_NAME, &args, amount)
            .await
    }

    pub async fn unwrap<C: SendTx, W: Wallet>(
        client: &C,
        wallet: &W,
        amount: Balance,
    ) -> Result<C::Output> {
        let log = DEFAULT.new(o!(
            "function" => "unwrap_near",
            "amount" => amount,
        ));
        info!(log, "unwrapping native token");

        const METHOD_NAME: &str = "near_withdraw";

        let token = WNEAR_TOKEN.clone();
        let args = json!({
            "amount": U128(amount),
        });

        let deposit = 1; // minimum deposit
        let signer = wallet.signer();

        client
            .exec_contract(signer, token.as_id(), METHOD_NAME, &args, deposit)
            .await
    }
}

pub async fn deposit<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    amount: Balance,
) -> Result<C::Output> {
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
    let signer = wallet.signer();

    client
        .exec_contract(signer, token.as_id(), METHOD_NAME, &args, deposit)
        .await
}

pub async fn get_deposits<C: ViewContract>(
    client: &C,
    account: &AccountId,
) -> Result<HashMap<TokenAccount, U128>> {
    let log = DEFAULT.new(o!(
        "function" => "get_deposits",
        "account" => format!("{}", account),
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "get_deposits";
    let args = json!({
        "account_id": account,
    });

    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let deposits: HashMap<TokenAccount, U128> = serde_json::from_slice(&result.result)?;
    info!(log, "deposits"; "deposits" => ?deposits);
    Ok(deposits)
}

pub async fn withdraw<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    amount: Balance,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "withdraw",
        "token" => format!("{}", token),
        "amount" => amount,
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "withdraw";

    let args = json!({
        "token_id": token,
        "amount": U128(amount),
        "skip_unwrap_near": false,
    });

    let deposit = 1; // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit)
        .await
}

pub async fn register_tokens<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "register_tokens",
        "tokens" => format!("{:?}", tokens),
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "register_tokens";
    let args = json!({
        "token_ids": tokens
    });

    let deposit = 1; // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit)
        .await
}

pub async fn unregister_tokens<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "unregister_tokens",
        "tokens" => format!("{:?}", tokens),
    ));
    info!(log, "entered");

    const METHOD_NAME: &str = "unregister_tokens";
    let args = json!({
        "token_ids": tokens
    });

    let deposit = 1; // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_primitives::views::CallResult;

    struct MockClient(HashMap<TokenAccount, U128>);

    impl ViewContract for MockClient {
        async fn view_contract<T>(&self, _: &AccountId, _: &str, _: &T) -> Result<CallResult>
        where
            T: ?Sized + serde::Serialize,
        {
            Ok(CallResult {
                result: serde_json::to_vec(&self.0)?,
                logs: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_get_deposits() {
        let token: TokenAccount = "wrap.testnet".parse().unwrap();
        let account = "app.zaciraci.testnet".parse().unwrap();
        let map = vec![(token.clone(), U128(100))].into_iter().collect();
        let client = MockClient(map);
        let result = get_deposits(&client, &account).await;
        assert!(result.is_ok());
        let deposits = result.unwrap();
        assert!(!deposits.is_empty());
        assert!(deposits.contains_key(&token));
    }
}
