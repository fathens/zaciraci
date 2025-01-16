#![allow(dead_code)]

use crate::jsonrpc;
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account;
use crate::wallet;
use crate::Result;
use near_primitives::types::Balance;
use near_sdk::NearToken;
use num_traits::Zero;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_REQUIRED_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const MINIMUM_NATIVE_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const INTERVAL_OF_PUTBACK: u64 = 24 * 60 * 60;

static LAST_PUTBACK: AtomicU64 = AtomicU64::new(0);

fn is_time_to_putback() -> bool {
    let last = LAST_PUTBACK.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > INTERVAL_OF_PUTBACK
}

fn update_last_putback() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_PUTBACK.store(now, Ordering::Relaxed);
}

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_value = {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_BALANCE
        } else {
            max
        }
    };
    info!(log, "Starting balances";
        "required_value" => %required_value,
    );

    let wrapped_balance = balance_of_start_token().await?;
    info!(log, "comparing";
        "wrapped_balance" => wrapped_balance,
    );

    if wrapped_balance < required_value {
        refill(required_value - wrapped_balance).await?;
    } else if (required_value * 10) < required_value {
        harvest().await?;
    }
    Ok(())
}

async fn balance_of_start_token() -> Result<Balance> {
    let account = wallet::WALLET.account_id();
    let token = &*token_account::START_TOKEN;
    let deposits = deposit::get_deposits(account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill(want: Balance) -> Result<()> {
    let native_balance = jsonrpc::get_native_amount().await?;
    let avalable = native_balance - MINIMUM_NATIVE_BALANCE;
    let amount = want.min(avalable);

    let token = &*token_account::START_TOKEN;
    deposit::deposit(token, amount).await
}

async fn harvest() -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_time_to_putback() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK - 1, Ordering::Relaxed);
        assert!(is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK, Ordering::Relaxed);
        assert!(!is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK + 1, Ordering::Relaxed);
        assert!(!is_time_to_putback());
        LAST_PUTBACK.store(now - INTERVAL_OF_PUTBACK + 2, Ordering::Relaxed);
        assert!(!is_time_to_putback());
    }

    #[test]
    fn test_update_last_putback() {
        LAST_PUTBACK.store(0, Ordering::Relaxed);
        update_last_putback();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(now, LAST_PUTBACK.load(Ordering::Relaxed));
    }
}
