#![allow(dead_code)]

use crate::jsonrpc::TxHash;
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use crate::types::{MicroNear, MilliNear};
use crate::wallet;
use crate::Result;
use crate::{config, jsonrpc};
use anyhow::anyhow;
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
    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|err| panic!("{}", err));
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

    let token = WNEAR_TOKEN.clone();

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
            tokio::spawn(async move {
                match harvest(&WNEAR_TOKEN, wrapped_balance - upper, upper).await {
                    Ok(_) => info!(log, "successfully harvested"),
                    Err(err) => warn!(log, "failed to harvest: {}", err),
                };
            });
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
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => format!("{}", want),
    ));
    let account = wallet::WALLET.account_id();
    let wrapped_balance = deposit::wnear::balance_of(account).await?;
    let log = log.new(o!(
        "wrapped_balance" => format!("{}", wrapped_balance),
    ));
    debug!(log, "checking");
    if wrapped_balance < want {
        let wrapping = want - wrapped_balance;
        let native_balance = jsonrpc::get_native_amount(account).await?;
        let log = log.new(o!(
            "native_balance" => format!("{}", native_balance),
            "wrapping" => format!("{}", wrapping),
        ));
        debug!(log, "checking");
        let available = native_balance
            .checked_sub(MINIMUM_NATIVE_BALANCE)
            .unwrap_or_default();
        if available < wrapping {
            return Err(anyhow!(
                "Insufficient balance: required: {:?}, native_balance {}, {:?}, {:?}",
                MilliNear::from_yocto(want),
                native_balance,
                MilliNear::from_yocto(native_balance),
                MicroNear::from_yocto(native_balance),
            ));
        }
        info!(log, "wrapping");
        deposit::wnear::wrap(wrapping)
            .await?
            .wait_for_success(account)
            .await?;
    }
    info!(log, "refilling";
        "amount" => %want,
    );
    deposit::deposit(&WNEAR_TOKEN, want)
        .await?
        .wait_for_success(account)
        .await?;
    Ok(())
}

async fn harvest(token: &TokenAccount, withdraw: Balance, required: Balance) -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "balances.harvest",
        "withdraw" => format!("{}", withdraw),
        "required" => format!("{}", required),
    ));
    info!(log, "withdrawing";
        "token" => %token,
    );
    deposit::withdraw(token, withdraw)
        .await?
        .wait_for_success(wallet::WALLET.account_id())
        .await?;
    let account = wallet::WALLET.account_id();
    let native_balance = jsonrpc::get_native_amount(account).await?;
    let upper = required << 4;
    info!(log, "checking";
        "native_balance" => %native_balance,
        "upper" => %upper,
    );
    if upper < native_balance && is_time_to_harvest() {
        let amount = native_balance - upper;
        let target = &*HARVEST_ACCOUNT;
        info!(log, "harvesting";
            "target" => %target,
            "amount" => %amount,
        );
        let signer = wallet::WALLET.signer();
        jsonrpc::transfer_native_token(signer, target, amount)
            .await?
            .wait_for_success(account)
            .await?;
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
