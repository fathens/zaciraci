use crate::Result;
use crate::config;
use crate::jsonrpc::{AccountInfo, SendTx, SentTx, ViewContract};
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::WNEAR_TOKEN;
use crate::wallet::Wallet;
use common::types::NearAmount;
use common::types::TokenAccount;
use logging::*;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

static DEFAULT_REQUIRED_BALANCE: Lazy<NearToken> = Lazy::new(|| NearToken::from_near(1));
#[cfg(test)]
const MINIMUM_NATIVE_BALANCE: NearToken = NearToken::from_near(1);

static LAST_HARVEST: AtomicU64 = AtomicU64::new(0);

fn harvest_account() -> AccountId {
    let value = config::get("HARVEST_ACCOUNT_ID").expect("HARVEST_ACCOUNT_ID config is required");
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse HARVEST_ACCOUNT_ID `{value}`: {err}"))
}

fn trade_account_reserve() -> NearToken {
    let yocto = config::get("TRADE_ACCOUNT_RESERVE")
        .ok()
        .and_then(|v| v.parse::<NearAmount>().ok())
        .unwrap_or_else(|| "10".parse().expect("valid NearAmount literal"))
        .to_yocto()
        .to_u128();
    NearToken::from_yoctonear(yocto)
}

fn harvest_interval() -> u64 {
    config::get("HARVEST_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(86400)
}

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after UNIX epoch")
        .as_secs();
    now - last > harvest_interval()
}

fn update_last_harvest() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after UNIX epoch")
        .as_secs();
    LAST_HARVEST.store(now, Ordering::Relaxed);
}

/// 残高上限乗数を適用するヘルパー関数
fn multiply_by_balance_multiplier(token: NearToken) -> NearToken {
    let multiplier: u128 = config::get("HARVEST_BALANCE_MULTIPLIER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(128);
    NearToken::from_yoctonear(token.as_yoctonear().saturating_mul(multiplier))
}

pub async fn start<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    required_balance: Option<NearToken>,
) -> Result<NearToken>
where
    C: AccountInfo + SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_balance = required_balance.unwrap_or_else(|| {
        let max = get_history()
            .read()
            .expect("history lock is read-only; poisoning is impossible")
            .inputs
            .max();
        if max.is_zero() {
            *DEFAULT_REQUIRED_BALANCE
        } else {
            NearToken::from_yoctonear(max)
        }
    });
    trace!(log, "Starting balances";
        "required_balance" => required_balance.as_yoctonear(),
    );

    // Note: storage deposit check is now performed once in trade::start
    // via ensure_ref_storage_setup, so we skip it here to reduce RPC calls

    let deposited_wnear = balance_of_start_token(client, wallet, token).await?;
    trace!(log, "comparing";
        "deposited_wnear" => deposited_wnear.as_yoctonear(),
        "required_balance" => required_balance.as_yoctonear(),
    );

    if deposited_wnear < required_balance {
        let shortage = required_balance.saturating_sub(deposited_wnear);
        refill(client, wallet, shortage).await?;
        // refill後の残高を再取得して返す
        let new_balance = balance_of_start_token(client, wallet, token).await?;
        Ok(new_balance)
    } else {
        let upper = multiply_by_balance_multiplier(required_balance); // 乗数倍
        if upper < deposited_wnear {
            let withdraw_amount = deposited_wnear.saturating_sub(upper);
            match harvest(
                client,
                wallet,
                &WNEAR_TOKEN,
                withdraw_amount,
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
) -> Result<NearToken>
where
    C: ViewContract,
    W: Wallet,
{
    let account = wallet.account_id();
    let deposits = deposit::get_deposits(client, account).await?;
    let yocto = deposits.get(token).map(|u| u.0).unwrap_or_default();
    Ok(NearToken::from_yoctonear(yocto))
}

async fn refill<C, W>(client: &C, wallet: &W, want: NearToken) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => want.as_yoctonear(),
    ));
    let account = wallet.account_id();
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;
    let log = log.new(o!(
        "wrapped_balance" => wrapped_balance.as_yoctonear(),
    ));
    trace!(log, "checking");

    let actual_wrapping = if wrapped_balance < want {
        let wrapping = want.saturating_sub(wrapped_balance);
        let native_balance = client.get_native_amount(account).await?;

        let minimum_native_balance = trade_account_reserve();

        let log = log.new(o!(
            "native_balance" => native_balance.as_yoctonear(),
            "wrapping" => wrapping.as_yoctonear(),
            "minimum_native_balance" => minimum_native_balance.as_yoctonear(),
        ));
        trace!(log, "checking");
        let available = native_balance.saturating_sub(minimum_native_balance);

        let amount = if available < wrapping {
            debug!(log, "insufficient balance, using maximum available";
                "available" => available.as_yoctonear(),
                "wanted" => wrapping.as_yoctonear(),
            );
            available
        } else {
            wrapping
        };

        if amount.as_yoctonear() > 0 {
            trace!(log, "wrapping";
                "amount" => amount.as_yoctonear(),
            );
            deposit::wnear::wrap(client, wallet, amount)
                .await?
                .wait_for_success()
                .await?;
        }
        amount
    } else {
        NearToken::from_yoctonear(0)
    };

    let total_deposit = wrapped_balance.saturating_add(actual_wrapping);
    if total_deposit.as_yoctonear() > 0 {
        debug!(log, "refilling";
            "amount" => total_deposit.as_yoctonear(),
        );
        deposit::deposit(client, wallet, &WNEAR_TOKEN, total_deposit)
            .await?
            .wait_for_success()
            .await?;
    } else {
        trace!(log, "no amount to deposit")
    }
    Ok(())
}

/// 投資額全額を REF Finance にデポジット（初期ポートフォリオ構築用）
pub async fn deposit_wrap_near_to_ref<C, W>(client: &C, wallet: &W, amount: NearToken) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "deposit_wrap_near_to_ref",
        "amount" => amount.as_yoctonear(),
    ));

    let account = wallet.account_id();

    // REF Finance に既にデポジット済みの残高を確認
    let deposited_balance = balance_of_start_token(client, wallet, &WNEAR_TOKEN).await?;

    trace!(log, "checking existing balances";
        "deposited_in_ref" => deposited_balance.as_yoctonear(),
        "required" => amount.as_yoctonear()
    );

    if deposited_balance >= amount {
        trace!(
            log,
            "sufficient balance already deposited, no action needed"
        );
        return Ok(());
    }

    // 不足分を計算
    let shortage = amount.saturating_sub(deposited_balance);

    // wrap.near の現在残高を確認
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;

    trace!(log, "current wrap.near balance"; "wrapped_balance" => wrapped_balance.as_yoctonear());

    // 不足分を wrap.near から調達
    if wrapped_balance < shortage {
        // さらに NEAR を wrap する必要がある
        let wrapping = shortage.saturating_sub(wrapped_balance);
        let native_balance = client.get_native_amount(account).await?;

        let minimum_native_balance = trade_account_reserve();

        let available = native_balance.saturating_sub(minimum_native_balance);

        let actual_wrapping = if available < wrapping {
            info!(log, "insufficient balance, wrapping maximum available";
                "available" => available.as_yoctonear(),
                "wanted" => wrapping.as_yoctonear(),
            );
            available
        } else {
            wrapping
        };

        if actual_wrapping.as_yoctonear() > 0 {
            trace!(log, "wrapping NEAR to wrap.near"; "amount" => actual_wrapping.as_yoctonear());
            deposit::wnear::wrap(client, wallet, actual_wrapping)
                .await?
                .wait_for_success()
                .await?;
        }
    }

    // wrap.near の最終残高を確認して、デポジット可能な量を決定
    let final_wrapped_balance = deposit::wnear::balance_of(client, account).await?;

    if final_wrapped_balance.as_yoctonear() == 0 {
        return Err(anyhow::anyhow!("No wrap.near balance available to deposit"));
    }

    // 不足分と実際の残高の少ない方をデポジット
    let deposit_amount = if shortage < final_wrapped_balance {
        shortage
    } else {
        final_wrapped_balance
    };

    trace!(log, "depositing wrap.near to REF Finance";
        "shortage" => shortage.as_yoctonear(),
        "available" => final_wrapped_balance.as_yoctonear(),
        "depositing" => deposit_amount.as_yoctonear()
    );

    deposit::deposit(client, wallet, &WNEAR_TOKEN, deposit_amount)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "deposit completed successfully"; "deposited" => deposit_amount.as_yoctonear());
    Ok(())
}

pub async fn harvest<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    withdraw: NearToken,
    required: NearToken,
) -> Result<()>
where
    C: AccountInfo + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.harvest",
        "withdraw" => withdraw.as_yoctonear(),
        "required" => required.as_yoctonear(),
    ));
    info!(log, "withdrawing";
        "token" => %token,
    );

    let minimum_native_balance = trade_account_reserve();

    let account = wallet.account_id();
    let before_withdraw = client.get_native_amount(account).await?;
    let added = if before_withdraw < minimum_native_balance || is_time_to_harvest() {
        deposit::withdraw(client, wallet, token, withdraw)
            .await?
            .wait_for_success()
            .await?;
        withdraw
    } else {
        NearToken::from_yoctonear(0)
    };
    let native_balance = before_withdraw.saturating_add(added);
    let upper = multiply_by_balance_multiplier(required); // 乗数倍
    trace!(log, "checking";
        "native_balance" => native_balance.as_yoctonear(),
        "upper" => upper.as_yoctonear(),
    );
    if upper < native_balance && is_time_to_harvest() {
        let amount = native_balance.saturating_sub(upper);
        let target = &harvest_account();
        info!(log, "harvesting";
            "target" => %target,
            "amount" => amount.as_yoctonear(),
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
