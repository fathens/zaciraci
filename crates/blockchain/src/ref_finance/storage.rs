use crate::Result;
use crate::jsonrpc::{SendTx, SentTx, ViewContract};
use crate::ref_finance::{CONTRACT_ADDRESS, deposit};
use crate::wallet::Wallet;
use common::types::TokenAccount;
use logging::*;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct StorageBalanceBounds {
    pub min: U128,
    pub max: Option<U128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct StorageBalance {
    pub total: U128,
    pub available: U128,
}

pub async fn check_bounds<C: ViewContract>(client: &C) -> Result<StorageBalanceBounds> {
    let log = DEFAULT.new(o!("function" => "storage::check_bounds"));
    const METHOD_NAME: &str = "storage_balance_bounds";
    let args = json!({});
    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let bounds: StorageBalanceBounds = serde_json::from_slice(&result.result)?;
    trace!(log, "bounds"; "min" => ?bounds.min, "max" => ?bounds.max);
    Ok(bounds)
}

pub async fn deposit<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    value: NearToken,
    registration_only: bool,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!("function" => "storage::deposit"));
    const METHOD_NAME: &str = "storage_deposit";
    let args = json!({
        "registration_only": registration_only,
    });
    let signer = wallet.signer();
    info!(log, "depositing";
        "value" => value.as_yoctonear(),
        "signer" => ?signer.account_id,
    );

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, value)
        .await
}

pub async fn balance_of<C: ViewContract>(
    client: &C,
    account: &AccountId,
) -> Result<Option<StorageBalance>> {
    let log = DEFAULT.new(o!("function" => "storage::balance_of"));
    const METHOD_NAME: &str = "storage_balance_of";
    let args = json!({
        "account_id": account,
    });
    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let balance: Option<StorageBalance> = serde_json::from_slice(&result.result)?;
    if let Some(b) = &balance {
        trace!(log, "balance";
            "total" => ?b.total,
            "available" => ?b.available,
        );
    } else {
        trace!(log, "no balance");
    }
    Ok(balance)
}

/// REF Finance のストレージセットアップを確認し、必要に応じて初期化・掃除・top-up を実行する
///
/// planner::plan() で算出した計画に基づき、以下を実行する:
/// 1. 未登録ならアカウント初期登録（storage_deposit）
/// 2. ゼロ残高かつ keep に含まれない旧トークンを unregister（チャンク最大 10）
/// 3. unregister 後の実際の available で top-up 額を再計算
/// 4. top-up が上限を超える場合はエラー
/// 5. 不足があれば storage_deposit で top-up
/// 6. 未登録の必要トークンを register_tokens
///
/// # TOCTOU に関する注記
/// ステップ 2 では unregister 前に deposits を再取得してゼロ残高を再検証するが、
/// 再取得〜送信間には微小な時間窓が残る。REF Finance コントラクト側が
/// 非ゼロ残高の unregister を拒否するため資金損失にはならない。
/// 失敗した場合は warn ログを出して続行し、ステップ 3 で balance_of を再取得して
/// top-up 額を実測値から再計算するため、最終的な整合性は保たれる。
pub async fn ensure_ref_storage_setup<C, W>(
    client: &C,
    wallet: &W,
    needed_tokens: &[TokenAccount],
    keep: &[TokenAccount],
    max_top_up_yoctonear: u128,
) -> Result<()>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "storage::ensure_ref_storage_setup"));
    let account = wallet.account_id();

    info!(log, "ref storage ensure start";
        "account" => %account,
        "requested" => needed_tokens.len(),
        "keep" => keep.len(),
    );

    // 1. storage_balance_of でアカウント状態を確認、未登録ならアカウント初期登録
    let maybe_balance = balance_of(client, account).await?;
    if maybe_balance.is_none() {
        info!(
            log,
            "account not registered, performing initial storage deposit"
        );
        let bounds = check_bounds(client).await?;
        deposit(
            client,
            wallet,
            NearToken::from_yoctonear(bounds.min.0),
            false,
        )
        .await?
        .wait_for_success()
        .await?;
        info!(log, "initial storage deposit completed"; "amount" => bounds.min.0);
    }

    // 2. snapshot を取得して planner で計画を立てる
    let balance = balance_of(client, account)
        .await?
        .ok_or_else(|| anyhow::anyhow!("storage balance disappeared after initial deposit"))?;
    let deposits = deposit::get_deposits(client, account).await?;
    let bounds = check_bounds(client).await?;

    debug!(log, "storage snapshot";
        "total" => balance.total.0,
        "available" => balance.available.0,
        "deposits" => deposits.len(),
        "min_bound" => bounds.min.0,
    );

    let snapshot = planner::StorageSnapshot {
        balance,
        deposits,
        bounds,
    };

    // deposits が空（初回登録直後等）の場合は planner をスキップして直接 register
    let p = match planner::plan(&snapshot, needed_tokens, keep) {
        Ok(p) => p,
        Err(planner::PlanError::EmptyDeposits) => {
            debug!(log, "no existing deposits, registering tokens directly");
            if !needed_tokens.is_empty() {
                deposit::register_tokens(client, wallet, needed_tokens)
                    .await?
                    .wait_for_success()
                    .await?;
                info!(log, "tokens registered"; "count" => needed_tokens.len());
            }
            return Ok(());
        }
        Err(e) => {
            return Err(anyhow::anyhow!("storage planner error: {}", e));
        }
    };

    info!(log, "storage plan";
        "unregister" => p.to_unregister.len(),
        "register" => p.to_register.len(),
        "needed" => p.needed,
        "initial_top_up" => p.top_up.as_yoctonear(),
    );

    // 3. ゼロ残高の旧トークンを unregister（TOCTOU 再検証 + チャンク分割）
    if !p.to_unregister.is_empty() {
        info!(log, "unregister stale tokens"; "count" => p.to_unregister.len());

        // TOCTOU ガード: 直前に再取得して amount == 0 && ∉ keep を再確認
        let fresh_deposits = deposit::get_deposits(client, account).await?;
        let verified: Vec<TokenAccount> = p
            .to_unregister
            .iter()
            .filter(|token| {
                fresh_deposits
                    .get(*token)
                    .is_some_and(|amount| amount.0 == 0)
                    && !keep.contains(token)
            })
            .cloned()
            .collect();

        let dropped = p.to_unregister.len() - verified.len();
        if dropped > 0 {
            debug!(log, "unregister revalidated";
                "candidates" => verified.len(),
                "dropped_by_toctou" => dropped,
            );
        }

        // REF Finance の unregister_tokens は 1 トランザクションあたりのガスリミットにより
        // 大量トークンを一度に処理できないため、10 トークンずつ分割する。
        const CHUNK_SIZE: usize = 10;
        let total_chunks = verified.len().div_ceil(CHUNK_SIZE);
        let mut ok_count = 0usize;
        let mut fail_count = 0usize;
        for (i, chunk) in verified.chunks(CHUNK_SIZE).enumerate() {
            match deposit::unregister_tokens(client, wallet, chunk).await {
                Ok(sent) => match sent.wait_for_success().await {
                    Ok(_) => {
                        info!(log, "unregister chunk";
                            "chunk_idx" => i,
                            "chunk_total" => total_chunks,
                            "size" => chunk.len(),
                        );
                        ok_count += chunk.len();
                    }
                    Err(e) => {
                        warn!(log, "unregister chunk failed";
                            "chunk_idx" => i,
                            "tokens" => ?chunk,
                            "error" => %e,
                        );
                        fail_count += chunk.len();
                    }
                },
                Err(e) => {
                    warn!(log, "unregister chunk failed";
                        "chunk_idx" => i,
                        "tokens" => ?chunk,
                        "error" => %e,
                    );
                    fail_count += chunk.len();
                }
            }
        }
        info!(log, "unregister done"; "unregistered" => ok_count, "failed" => fail_count);
    }

    // 4. unregister 後の実際の available で top-up 額を再計算
    // unregister で available が needed 以上に増えた場合、saturating_sub により
    // actual_top_up = 0 となる。これは「top-up 不要」という正しい動作。
    let new_balance = balance_of(client, account)
        .await?
        .ok_or_else(|| anyhow::anyhow!("storage balance disappeared after unregister"))?;
    let new_available = new_balance.available.0;
    let actual_top_up = p.needed.saturating_sub(new_available);

    debug!(log, "top-up recalculated after unregister";
        "needed" => p.needed,
        "new_available" => new_available,
        "actual_top_up" => actual_top_up,
    );

    // 5. top-up が上限を超える場合はエラー
    let max_top_up = NearToken::from_yoctonear(max_top_up_yoctonear);
    if actual_top_up > max_top_up.as_yoctonear() {
        return Err(anyhow::anyhow!(
            "ref storage top-up {} yocto exceeds cap {} yocto",
            actual_top_up,
            max_top_up.as_yoctonear(),
        ));
    }

    // 6. top-up
    if actual_top_up > 0 {
        let top_up_amount = NearToken::from_yoctonear(actual_top_up);
        warn!(log, "ref storage top-up";
            "wallet" => %account,
            "amount" => actual_top_up,
            "available_before" => new_available,
            "cap" => max_top_up.as_yoctonear(),
        );
        deposit(client, wallet, top_up_amount, false)
            .await?
            .wait_for_success()
            .await?;
    }

    // 7. register_tokens
    if !p.to_register.is_empty() {
        info!(log, "registering tokens"; "count" => p.to_register.len());
        deposit::register_tokens(client, wallet, &p.to_register)
            .await?
            .wait_for_success()
            .await?;
        info!(log, "tokens registered"; "count" => p.to_register.len());
    }

    Ok(())
}

pub(crate) mod planner;

#[cfg(test)]
mod tests;
