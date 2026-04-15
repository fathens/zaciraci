use crate::Result;
use crate::jsonrpc::{SendTx, SentTx, ViewContract};
use crate::ref_finance::token_account::WNEAR_TOKEN;
use crate::ref_finance::{CONTRACT_ADDRESS, deposit};
use crate::wallet::Wallet;
use common::types::TokenAccount;
use logging::*;
use near_sdk::json_types::U128;
use near_sdk::{AccountId, NearToken};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use tokio::sync::Mutex as AsyncMutex;

/// 基軸通貨 WNEAR のみを保持対象とする keep list を作る。
///
/// 裁定取引や単発スワップなど、毎回トークン構成が変わるユースケースで、
/// WNEAR 以外のゼロ残高トークンを unregister してよい場合に使う。
pub fn keep_wnear_only() -> Vec<TokenAccount> {
    vec![WNEAR_TOKEN.clone()]
}

/// `ref_storage_max_top_up_yoctonear` を `NearToken` ドメイン型に変換して返す helper。
///
/// trade/arbitrage から `ensure_ref_storage_setup` を呼ぶ際に繰り返し書かれていた
/// `NearToken::from_yoctonear(cfg.ref_storage_max_top_up_yoctonear())` を一箇所に集約する。
/// `common` crate に `near-sdk` 依存を追加しないため、ここ（blockchain crate）に置く。
pub fn max_top_up_from_config(cfg: &dyn common::config::ConfigAccess) -> NearToken {
    NearToken::from_yoctonear(cfg.ref_storage_max_top_up_yoctonear())
}

/// ポートフォリオ運用中のトークン + 基軸通貨 WNEAR を保持対象とする keep list を作る。
///
/// 次サイクルで再利用する予定のトークンを unregister してしまわないように使う。
/// WNEAR が `tokens` に含まれていなくても必ず追加される。
pub fn keep_with_portfolio(tokens: &[TokenAccount]) -> Vec<TokenAccount> {
    let mut keep = tokens.to_vec();
    if !keep.contains(&WNEAR_TOKEN) {
        keep.push(WNEAR_TOKEN.clone());
    }
    keep
}

/// 同一アカウントでの `ensure_ref_storage_setup` の重複実行を直列化するためのロックマップ。
///
/// **CRITICAL: single-process invariant** — このロックは **同一プロセス内** のみ直列化する。
/// 同じ `ROOT_ACCOUNT_ID` を握る backend を複数プロセス/コンテナで並行起動した場合、
/// 二重 initial deposit や二重 top-up の race に対する保護は失われる。backend は
/// singleton として運用することが前提。詳細は `crates/backend/src/main.rs` の該当 doc を参照。
///
/// クロスプロセスの排他が必要になった場合は `persistence::pg_advisory_lock` 等を介した
/// follow-up で対応する（本モジュールに `trait CrossProcessLock` を導入し backend で DI する
/// 設計を想定）。
///
/// backend では `trade::run` と `arbitrage::run` が同一ウォレットで並行起動されるため、
/// snapshot → unregister → top-up → register の一連が atomic でないと二重 initial deposit
/// や二重 top-up が発生する。account 単位で tokio::sync::Mutex を引き当て、
/// `ensure_ref_storage_setup` 全体を逐次化する。
///
/// 外側 `StdMutex` はマップ更新のみ保護（await を跨がない短命ロック）、内側
/// `tokio::sync::Mutex` がアカウント単位の await 跨ぎロック。異なる account 間は並行可。
static REF_STORAGE_LOCKS: LazyLock<StdMutex<HashMap<AccountId, Arc<AsyncMutex<()>>>>> =
    LazyLock::new(|| StdMutex::new(HashMap::new()));

/// `account` 用の非同期ロックを取得する。
///
/// `StdMutex` ガードは本関数のスコープ内で drop されるため、呼び出し元が返り値の
/// `Arc<AsyncMutex<()>>` を await するとき `clippy::await_holding_lock` には触れない。
fn lock_for(account: &AccountId) -> Arc<AsyncMutex<()>> {
    // `REF_STORAGE_LOCKS` の臨界区間は `HashMap::entry` の挿入/取得のみで、途中で panic
    // しても HashMap の不変条件は壊れない。ロック全アカウント共有のため poison が全体停止に
    // 直結するリスクを避け、`into_inner` で内部 HashMap をそのまま取り出して続行する。
    let mut map = REF_STORAGE_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.entry(account.clone())
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

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
    max_top_up: NearToken,
) -> Result<()>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    let log = DEFAULT.new(o!("function" => "storage::ensure_ref_storage_setup"));
    let account = wallet.account_id();

    // 呼び出し側の実装ミス（異常に多いトークンを 1 呼び出しで登録しようとする）を早期に検出。
    // planner 側の `PlanError::TooManyTokens` と対で動くが、planner をスキップする `None` 分岐
    // （deposits 空）でも効く位置に置くことで全経路で sanity guard が有効になる。
    //
    // strict `>` で判定することで planner 側の同演算子を維持する（ちょうど上限値は許容）。
    // release build でも多層防御が完結するよう、`debug_assert!` ではなく runtime Err を返す。
    //
    // **非対称性**: ここでは raw `needed_tokens.len()`（filter 前）を見る。
    // [`planner::plan`] は filtered `to_register.len()`（= raw − 既登録分）を見る。
    // raw を先に弾くことで、planner が呼ばれない `None` 経路も含めて境界を保証する。
    // 詳細は [`planner::MAX_REGISTER_PER_CYCLE`] の doc を参照。
    if needed_tokens.len() > planner::MAX_REGISTER_PER_CYCLE {
        return Err(anyhow::anyhow!(
            "needed_tokens ({}) exceeds MAX_REGISTER_PER_CYCLE ({})",
            needed_tokens.len(),
            planner::MAX_REGISTER_PER_CYCLE,
        ));
    }

    // 同一アカウントでの並行実行を直列化（二重 deposit/top-up 防止）。
    // std::sync::Mutex guard は lock_for の内部で drop されるため await 跨ぎ問題は発生しない。
    let mutex = lock_for(account);
    let _guard = mutex.lock().await;

    info!(log, "ref storage ensure start";
        "account" => %account,
        "requested" => needed_tokens.len(),
        "keep" => keep.len(),
    );

    // 1. storage_balance_of でアカウント状態を確認、未登録ならアカウント初期登録
    let maybe_balance = balance_of(client, account).await?;
    let initial_deposit = if maybe_balance.is_some() {
        NearToken::from_yoctonear(0)
    } else {
        info!(
            log,
            "account not registered, performing initial storage deposit"
        );
        let bounds = check_bounds(client).await?;
        let amount = NearToken::from_yoctonear(bounds.min.0);
        if amount > max_top_up {
            return Err(anyhow::anyhow!(
                "initial storage deposit {} yocto exceeds cap {} yocto",
                amount.as_yoctonear(),
                max_top_up.as_yoctonear(),
            ));
        }
        // `registration_only=true` で送金することで、REF Finance 側は必要量（= `bounds.min`）
        // ぴったりで登録し、超過分（= 0）のみを refund する。既に登録済みのアカウントだった
        // 場合は contract 仕様により全額 refund される（contract_spec.md §2.2 参照）ため、
        // stale view RPC での二重 deposit が起きても storage_balance が過剰に増えない。
        //
        // 本コードは `amount == bounds.min` を前提に `registration_only=true` を選んでいる。
        // 将来、初期登録時に min_bound 以上を確保したくなった場合は、`false` に戻すか、
        // refund ぶんを cap 会計から差し引く必要がある。
        deposit(client, wallet, amount, true)
            .await?
            .wait_for_success()
            .await?;
        info!(log, "initial storage deposit completed"; "amount" => amount.as_yoctonear());
        amount
    };

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

    // deposits が空（初回登録直後等）の場合は planner が `None` を返すため、
    // ここでは early return して planner をスキップし直接 register する。
    //
    // このパスは cap（`remaining_cap = max_top_up - initial_deposit`）の減算検証を
    // 通らずに `register_tokens` を呼ぶ。安全性は以下の前提に依存する:
    // - 初期 deposit の cap ガードはステップ 1 の `initial_deposit > max_top_up` で完結しており、
    //   このパスに到達する時点で `initial_deposit <= max_top_up` が保証されている。
    // - `register_tokens` は attached deposit 1 yocto のみで storage 資金を動かさない。
    //   将来 `register_tokens` が attached deposit を増やす変更を入れる場合は、ここでも
    //   cap 検証を追加する必要がある。
    // - `needed_tokens.len() <= MAX_REGISTER_PER_CYCLE` は関数先頭の debug_assert で
    //   sanity check 済み。
    //
    // 実装上の不変条件: この後のステップ 4-6 で扱う `actual_top_up` / `remaining_cap` /
    // top-up 実行は、以下の `p` スコープ内でのみ使用可能とすることで、cap 検証を
    // 迂回する新経路を構造的に生み出せないよう保っている。
    let Some(p) = planner::plan(&snapshot, needed_tokens, keep)? else {
        debug!(log, "no existing deposits, registering tokens directly");
        if !needed_tokens.is_empty() {
            deposit::register_tokens(client, wallet, needed_tokens)
                .await?
                .wait_for_success()
                .await?;
            info!(log, "tokens registered"; "count" => needed_tokens.len());
        }
        return Ok(());
    };

    info!(log, "storage plan";
        "unregister" => p.to_unregister.len(),
        "register" => p.to_register.len(),
        "needed" => p.pre_unregister_estimate.as_yoctonear(),
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
            let result = async {
                deposit::unregister_tokens(client, wallet, chunk)
                    .await?
                    .wait_for_success()
                    .await
            }
            .await;
            match result {
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
            }
        }
        info!(log, "unregister done"; "unregistered" => ok_count, "failed" => fail_count);
    }

    // 4. unregister 後の実際の available で top-up 額を再計算
    //
    // `p.pre_unregister_estimate` は planner が初期 snapshot から推定した値
    // （`planner::Plan::pre_unregister_estimate` の doc 参照）で、unregister で `deposits_len`
    // が減った影響は反映されていない。saturating_sub は `available` 増加のみ反映するため:
    //
    // - 過大評価（per_token が実際より大きく見積もられた）→ top-up 多め、安全側
    // - 過小評価（unregister 後 per_token が上昇し実需が増えた）→ top-up 不足で
    //   register_tokens がコントラクト拒否 → Err を上位へ。次サイクルで
    //   `balance_of` を再取得した新しい snapshot から planner が再計算するため
    //   self-healing する（資金損失なし）。
    //
    // unregister で `available` が `pre_unregister_estimate` 以上に増えた場合、saturating_sub
    // により actual_top_up = 0 となる。これは「top-up 不要」という正しい動作。
    let post_unregister_balance = balance_of(client, account)
        .await?
        .ok_or_else(|| anyhow::anyhow!("storage balance disappeared after unregister"))?;
    let post_unregister_available = post_unregister_balance.available.0;
    let actual_top_up = p
        .pre_unregister_estimate
        .saturating_sub(NearToken::from_yoctonear(post_unregister_available));

    debug!(log, "top-up recalculated after unregister";
        "needed" => p.pre_unregister_estimate.as_yoctonear(),
        "post_unregister_available" => post_unregister_available,
        "actual_top_up" => actual_top_up.as_yoctonear(),
    );

    // 5. 累積支出が上限を超える場合はエラー
    //
    // 初期 deposit を実行した場合、そのぶんを max_top_up から差し引いた残り枠で
    // top-up の可否を判定する。これにより単一呼び出しでの総消費 NEAR が
    // max_top_up を超えないことを保証する（初期 deposit と top-up の二重キャップ回避）。
    let remaining_cap = max_top_up.saturating_sub(initial_deposit);
    if actual_top_up > remaining_cap {
        return Err(anyhow::anyhow!(
            "ref storage top-up {} yocto exceeds remaining cap {} yocto \
             (max_top_up={}, initial_deposit={})",
            actual_top_up.as_yoctonear(),
            remaining_cap.as_yoctonear(),
            max_top_up.as_yoctonear(),
            initial_deposit.as_yoctonear(),
        ));
    }

    // 6. top-up
    if !actual_top_up.is_zero() {
        warn!(log, "ref storage top-up";
            "wallet" => %account,
            "amount" => actual_top_up.as_yoctonear(),
            "available_before" => post_unregister_available,
            "cap" => max_top_up.as_yoctonear(),
        );
        deposit(client, wallet, actual_top_up, false)
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

mod planner;

#[cfg(test)]
mod tests;
