use crate::Result;
use crate::config;
use crate::jsonrpc::{AccountInfo, SendTx, SentTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use crate::types::MilliNear;
use crate::wallet::Wallet;
use near_primitives::types::Balance;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_REQUIRED_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
#[cfg(test)]
const MINIMUM_NATIVE_BALANCE: Balance = NearToken::from_near(1).as_yoctonear(); // テスト用の定数
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

pub async fn start<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    required_balance: Option<Balance>,
) -> Result<Balance>
where
    C: AccountInfo + SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_balance = required_balance.unwrap_or_else(|| {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_BALANCE
        } else {
            max
        }
    });
    info!(log, "Starting balances";
        "required_balance" => %required_balance,
    );

    // Note: storage deposit check is now performed once in trade::start
    // via ensure_ref_storage_setup, so we skip it here to reduce RPC calls

    let deposited_wnear = balance_of_start_token(client, wallet, token).await?;
    info!(log, "comparing";
        "deposited_wnear" => ?deposited_wnear,
        "required_balance" => ?required_balance,
    );

    if deposited_wnear < required_balance {
        refill(client, wallet, required_balance - deposited_wnear).await?;
        Ok(deposited_wnear)
    } else {
        let upper = required_balance << 7; // 128倍
        if upper < deposited_wnear {
            match harvest(
                client,
                wallet,
                &WNEAR_TOKEN,
                deposited_wnear - upper,
                required_balance,
            )
            .await
            {
                Ok(_) => info!(log, "successfully harvested"),
                Err(err) => warn!(log, "failed to harvest: {}", err),
            }
        }
        Ok(deposited_wnear)
    }
}

async fn balance_of_start_token<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
) -> Result<Balance>
where
    C: ViewContract,
    W: Wallet,
{
    let account = wallet.account_id();
    let deposits = deposit::get_deposits(client, account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill<C, W>(client: &C, wallet: &W, want: Balance) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => format!("{}", want),
    ));
    let account = wallet.account_id();
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;
    let log = log.new(o!(
        "wrapped_balance" => format!("{}", wrapped_balance),
    ));
    debug!(log, "checking");

    let actual_wrapping = if wrapped_balance < want {
        let wrapping = want - wrapped_balance;
        let native_balance = client.get_native_amount(account).await?;

        // アカウント保護額を環境変数から取得（デフォルト10 NEAR）
        let minimum_native_balance = config::get("TRADE_ACCOUNT_RESERVE")
            .ok()
            .and_then(|v| v.parse::<u128>().ok())
            .map(|v| MilliNear::from_near(v).to_yocto())
            .unwrap_or_else(|| MilliNear::from_near(10).to_yocto());

        let log = log.new(o!(
            "native_balance" => format!("{}", native_balance),
            "wrapping" => format!("{}", wrapping),
            "minimum_native_balance" => format!("{}", minimum_native_balance),
        ));
        debug!(log, "checking");
        let available = native_balance
            .checked_sub(minimum_native_balance)
            .unwrap_or_default();

        let amount = if available < wrapping {
            info!(log, "insufficient balance, using maximum available";
                "available" => %available,
                "wanted" => %wrapping,
            );
            available
        } else {
            wrapping
        };

        if amount > 0 {
            info!(log, "wrapping";
                "amount" => %amount,
            );
            deposit::wnear::wrap(client, wallet, amount)
                .await?
                .wait_for_success()
                .await?;
        }
        amount
    } else {
        0
    };

    let total_deposit = wrapped_balance + actual_wrapping;
    if total_deposit > 0 {
        info!(log, "refilling";
            "amount" => %total_deposit,
        );
        deposit::deposit(client, wallet, &WNEAR_TOKEN, total_deposit)
            .await?
            .wait_for_success()
            .await?;
    } else {
        info!(log, "no amount to deposit")
    }
    Ok(())
}

/// 投資額全額を REF Finance にデポジット（初期ポートフォリオ構築用）
pub async fn deposit_wrap_near_to_ref<C, W>(client: &C, wallet: &W, amount: Balance) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "deposit_wrap_near_to_ref",
        "amount" => format!("{}", amount),
    ));

    // wrap.near の現在残高を確認
    let account = wallet.account_id();
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;

    info!(log, "current wrap.near balance"; "wrapped_balance" => wrapped_balance);

    if wrapped_balance < amount {
        // 不足分を wrap
        let wrapping = amount - wrapped_balance;
        let native_balance = client.get_native_amount(account).await?;

        let minimum_native_balance = config::get("TRADE_ACCOUNT_RESERVE")
            .ok()
            .and_then(|v| v.parse::<u128>().ok())
            .map(|v| MilliNear::from_near(v).to_yocto())
            .unwrap_or_else(|| MilliNear::from_near(10).to_yocto());

        let available = native_balance
            .checked_sub(minimum_native_balance)
            .unwrap_or_default();

        let actual_wrapping = if available < wrapping {
            info!(log, "insufficient balance, wrapping maximum available";
                "available" => %available,
                "wanted" => %wrapping,
            );
            available
        } else {
            wrapping
        };

        if actual_wrapping > 0 {
            info!(log, "wrapping NEAR to wrap.near"; "amount" => %actual_wrapping);
            deposit::wnear::wrap(client, wallet, actual_wrapping)
                .await?
                .wait_for_success()
                .await?;
        }
    }

    // wrap.near を REF にデポジット
    info!(log, "depositing wrap.near to REF Finance"; "amount" => %amount);
    deposit::deposit(client, wallet, &WNEAR_TOKEN, amount)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "deposit completed successfully");
    Ok(())
}

pub async fn harvest<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    withdraw: Balance,
    required: Balance,
) -> Result<()>
where
    C: AccountInfo + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.harvest",
        "withdraw" => format!("{}", withdraw),
        "required" => format!("{}", required),
    ));
    info!(log, "withdrawing";
        "token" => %token,
    );

    // アカウント保護額を環境変数から取得（デフォルト10 NEAR）
    let minimum_native_balance = config::get("TRADE_ACCOUNT_RESERVE")
        .ok()
        .and_then(|v| v.parse::<u128>().ok())
        .map(|v| MilliNear::from_near(v).to_yocto())
        .unwrap_or_else(|| MilliNear::from_near(10).to_yocto());

    let account = wallet.account_id();
    let before_withdraw = client.get_native_amount(account).await?;
    let added = if before_withdraw < minimum_native_balance || is_time_to_harvest() {
        deposit::withdraw(client, wallet, token, withdraw)
            .await?
            .wait_for_success()
            .await?;
        withdraw
    } else {
        0
    };
    let native_balance = before_withdraw + added;
    let upper = required << 7; // 128倍
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
        let signer = wallet.signer();
        client
            .transfer_native_token(signer, target, amount)
            .await?
            .wait_for_success()
            .await?;
        update_last_harvest()
    }
    Ok(())
}

#[cfg(test)]
mod tests;
