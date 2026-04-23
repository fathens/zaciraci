use crate::Result;
use crate::jsonrpc::{SendTx, ViewContract};
use crate::ref_finance::CONTRACT_ADDRESS;
use crate::wallet::Wallet;
use common::types::TokenAccount;
use logging::*;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde_json::json;
use std::collections::BTreeMap;

/// NEP-145 `storage_deposit` の `registration_only` フラグを表す自己説明的な enum。
///
/// ## 背景
///
/// REF Finance の `storage_deposit` は NEP-145 に準拠し、`registration_only`
/// ブール引数で 2 つの動作を切り替える:
///
/// - `true`: アカウント登録のみ行い、必要な `bounds.min` 分を受領して超過分を
///   refund する。`ensure_ref_storage_setup` step 1 の初回登録経路で使用。
/// - `false`: 指定額を account の storage balance に加算する（未登録なら同時に
///   register する）。step 6 の top-up 経路で使用。
///
/// ## なぜ enum なのか
///
/// step 1 の cap guard (`amount > max_top_up → Err`) は `registration_only=true`
/// 前提で成立しており、`false` に切り替えると超過分が `storage_balance` に
/// 吸収されて cap 会計が壊れる（contract_spec.md §2.2）。NEP-145 のプロトコル
/// 用語をそのまま型に落とし込むことで、呼び出し側が二つの分岐を取り違えるリスクを
/// 型レベルで抑止する（make illegal states unrepresentable 原則）。
///
/// `Plan::InitialRegister` / `Plan::Normal` variant や `NormalPlanArgs` 構造体と
/// 同じ方向の防御であり、`bool` を残すと分岐モデルに一貫性の穴が残る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepositMode {
    /// NEP-145 `registration_only = true`。
    /// アカウント登録のみを行い、`bounds.min` ちょうどを消費して余剰は refund される。
    /// 初回登録経路（`ensure_ref_storage_setup` step 1）で使用する。
    RegistrationOnly,
    /// NEP-145 `registration_only = false`。
    /// 指定額をアカウントの storage balance に加算する（未登録なら同時に register）。
    /// top-up 経路（`ensure_ref_storage_setup` step 6）で使用する。
    DepositWithRegistration,
}

impl DepositMode {
    /// NEP-145 `registration_only` ブールへシリアライズする。
    ///
    /// JSON 引数組み立て専用のヘルパ。呼び出し側コードでは `DepositMode` のまま
    /// 受け渡し、この関数は `storage::deposit` の JSON 組み立て箇所でのみ使う。
    pub const fn registration_only(self) -> bool {
        matches!(self, Self::RegistrationOnly)
    }
}

pub mod wnear {
    use crate::Result;
    use crate::jsonrpc::{SendTx, ViewContract};
    use crate::ref_finance::token_account::WNEAR_TOKEN;
    use crate::wallet::Wallet;
    use logging::*;
    use near_sdk::json_types::U128;
    use near_sdk::{AccountId, NearToken};
    use serde_json::json;

    pub async fn balance_of<C: ViewContract>(client: &C, account: &AccountId) -> Result<NearToken> {
        let log = DEFAULT.new(o!(
            "function" => "balance_of",
            "account" => format!("{}", account),
        ));
        trace!(log, "entered");

        const METHOD_NAME: &str = "ft_balance_of";
        let args = json!({
            "account_id": account,
        });

        let result = client
            .view_contract(WNEAR_TOKEN.as_account_id(), METHOD_NAME, &args)
            .await?;
        let balance: U128 = serde_json::from_slice(&result.result)?;
        Ok(NearToken::from_yoctonear(balance.0))
    }

    pub async fn wrap<C: SendTx, W: Wallet>(
        client: &C,
        wallet: &W,
        amount: NearToken,
    ) -> Result<C::Output> {
        let log = DEFAULT.new(o!(
            "function" => "wrap_near",
            "amount" => amount.as_yoctonear(),
        ));
        trace!(log, "wrapping native token");

        const METHOD_NAME: &str = "near_deposit";

        let args = json!({});
        let signer = wallet.signer();

        client
            .exec_contract(
                signer,
                WNEAR_TOKEN.as_account_id(),
                METHOD_NAME,
                &args,
                amount,
            )
            .await
    }

    pub async fn unwrap<C: SendTx, W: Wallet>(
        client: &C,
        wallet: &W,
        amount: NearToken,
    ) -> Result<C::Output> {
        let log = DEFAULT.new(o!(
            "function" => "unwrap_near",
            "amount" => amount.as_yoctonear(),
        ));
        trace!(log, "unwrapping native token");

        const METHOD_NAME: &str = "near_withdraw";

        let args = json!({
            "amount": U128(amount.as_yoctonear()),
        });

        let deposit = NearToken::from_yoctonear(1); // minimum deposit
        let signer = wallet.signer();

        client
            .exec_contract(
                signer,
                WNEAR_TOKEN.as_account_id(),
                METHOD_NAME,
                &args,
                deposit,
            )
            .await
    }
}

pub async fn deposit<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    amount: NearToken,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "deposit",
        "token" => format!("{}", token),
        "amount" => amount.as_yoctonear(),
    ));
    trace!(log, "entered");

    const METHOD_NAME: &str = "ft_transfer_call";

    let args = json!({
        "receiver_id": CONTRACT_ADDRESS.clone(),
        "amount": U128(amount.as_yoctonear()),
        "msg": "",
    });

    let deposit = NearToken::from_yoctonear(1); // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, token.as_account_id(), METHOD_NAME, &args, deposit)
        .await
}

/// REF Finance に登録された account の deposit 一覧を取得する。
///
/// 戻り値は `BTreeMap` で決定的な iteration 順序を保証する。これにより
/// storage planner の unregister 候補選択が実行毎に再現可能になり、
/// ログ監査とテスト再現性が向上する。
pub async fn get_deposits<C: ViewContract>(
    client: &C,
    account: &AccountId,
) -> Result<BTreeMap<TokenAccount, U128>> {
    let log = DEFAULT.new(o!(
        "function" => "get_deposits",
        "account" => format!("{}", account),
    ));
    trace!(log, "entered");

    const METHOD_NAME: &str = "get_deposits";
    let args = json!({
        "account_id": account,
    });

    let result = client
        .view_contract(&CONTRACT_ADDRESS, METHOD_NAME, &args)
        .await?;

    let deposits: BTreeMap<TokenAccount, U128> = serde_json::from_slice(&result.result)?;
    trace!(log, "deposits"; "deposits" => ?deposits);
    Ok(deposits)
}

pub async fn withdraw<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    amount: NearToken,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "withdraw",
        "token" => format!("{}", token),
        "amount" => amount.as_yoctonear(),
    ));
    trace!(log, "entered");

    const METHOD_NAME: &str = "withdraw";

    let args = json!({
        "token_id": token,
        "amount": U128(amount.as_yoctonear()),
        "skip_unwrap_near": false,
    });

    let deposit = NearToken::from_yoctonear(1); // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit)
        .await
}

/// REF Finance に `token_ids` を登録する。
///
/// NOTE: `attached_deposit=1 yocto` は NEP-145 の `assert_one_yocto` 標準に基づく
/// external invariant。この値を変更する場合、`storage.rs` の cap 迂回経路
/// （`ensure_ref_storage_setup` の `None` arm）で `register_tokens` を cap 検証なしに
/// 呼ぶ前提が壊れるため、呼び出し側の cap 再計算ロジックの追加が必須。
pub async fn register_tokens<C: SendTx, W: Wallet>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<C::Output> {
    let log = DEFAULT.new(o!(
        "function" => "register_tokens",
        "tokens" => format!("{:?}", tokens),
    ));
    trace!(log, "entered");

    const METHOD_NAME: &str = "register_tokens";
    let args = json!({
        "token_ids": tokens
    });

    let deposit = NearToken::from_yoctonear(1); // minimum deposit
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
    trace!(log, "entered");

    const METHOD_NAME: &str = "unregister_tokens";
    let args = json!({
        "token_ids": tokens
    });

    let deposit = NearToken::from_yoctonear(1); // minimum deposit
    let signer = wallet.signer();

    client
        .exec_contract(signer, &CONTRACT_ADDRESS, METHOD_NAME, &args, deposit)
        .await
}

#[cfg(test)]
mod tests;
