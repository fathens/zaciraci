#![allow(dead_code)]

use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account;
use crate::ref_finance::token_account::TokenAccount;
use crate::wallet;
use crate::Result;
use crate::{config, jsonrpc};
use near_primitives::types::Balance;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_REQUIRED_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const MINIMUM_NATIVE_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const INTERVAL_OF_HARVEST: u64 = 24 * 60 * 60;

static LAST_HARVEST: AtomicU64 = AtomicU64::new(0);
static HARVEST_ACCOUNT: Lazy<AccountId> = Lazy::new(|| {
    let value = config::get("HARVEST_ACCOUNT").unwrap_or_else(|err| panic!("{}", err));
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse config `{}`: {}", value, err))
});

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > INTERVAL_OF_HARVEST
}

fn update_last_harvest() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST.store(now, Ordering::Relaxed);
}

pub async fn start() -> Result<(TokenAccount, Balance)> {
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_balance = {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_BALANCE
        } else {
            max
        }
    };
    info!(log, "Starting balances";
        "required_balance" => %required_balance,
    );

    let token = token_account::START_TOKEN.clone();

    let wrapped_balance = balance_of_start_token(&token).await?;
    info!(log, "comparing";
        "wrapped_balance" => wrapped_balance,
    );

    if wrapped_balance < required_balance {
        refill(required_balance - wrapped_balance).await?;
        Ok((token, wrapped_balance))
    } else {
        let upper = required_balance << 4;
        if upper < wrapped_balance {
            harvest(&token, wrapped_balance - upper, upper).await?;
        }
        Ok((token, upper))
    }
}

async fn balance_of_start_token(token: &TokenAccount) -> Result<Balance> {
    let account = wallet::WALLET.account_id();
    let deposits = deposit::get_deposits(account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill(want: Balance) -> Result<()> {
    let account = wallet::WALLET.account_id();
    let native_balance = jsonrpc::get_native_amount(account).await?;
    let amount = native_balance
        .checked_sub(MINIMUM_NATIVE_BALANCE)
        .unwrap_or_default()
        .min(want);

    let token = deposit::wrap_near(amount).await?;
    deposit::deposit(&token, amount).await
}

async fn harvest(token: &TokenAccount, withdraw: Balance, required: Balance) -> Result<()> {
    deposit::withdraw(token, withdraw).await?;
    let account = wallet::WALLET.account_id();
    let native_balance = jsonrpc::get_native_amount(account).await?;
    let upper = required << 4;
    if upper < native_balance && is_time_to_harvest() {
        let amount = native_balance - upper;
        let target = &*HARVEST_ACCOUNT;
        let signer = wallet::WALLET.signer();
        jsonrpc::transfer_native_token(signer, target, amount).await?;
        update_last_harvest()
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_time_to_harvest() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST - 1, Ordering::Relaxed);
        assert!(is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 1, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 2, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
    }

    #[test]
    fn test_update_last_harvest() {
        LAST_HARVEST.store(0, Ordering::Relaxed);
        update_last_harvest();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(now, LAST_HARVEST.load(Ordering::Relaxed));
    }
}
