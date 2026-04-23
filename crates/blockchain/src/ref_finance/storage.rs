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
///
/// # Absolute ceiling enforcement (defense-in-depth)
///
/// The resolved configured value is clipped to
/// [`common::config::REF_STORAGE_MAX_TOP_UP_ABSOLUTE_CEILING`] before it is
/// returned. This protects against cap bypass via `DB_STORE` write privilege
/// compromise: even if an attacker injects an extreme
/// `REF_STORAGE_MAX_TOP_UP_YOCTONEAR`, the effective cap passed into
/// `ensure_ref_storage_setup` cannot exceed the hard-coded ceiling.
///
/// When the clip engages (configured > ceiling) a `warn!` record is emitted
/// with both values so that any attempted bypass leaves an audit trail — silent
/// clips are forbidden. On the first resolution per process an `info!` record
/// captures the effective value so that operators can verify the startup-time
/// cap.
pub fn max_top_up_from_config(cfg: &dyn common::config::ConfigAccess) -> NearToken {
    let log = DEFAULT.new(o!("function" => "storage::max_top_up_from_config"));
    let configured = cfg.ref_storage_max_top_up_yoctonear();
    let ceiling = common::config::REF_STORAGE_MAX_TOP_UP_ABSOLUTE_CEILING;
    let effective = configured.min(ceiling);

    if configured > ceiling {
        warn!(log, "ref storage max top-up clipped to absolute ceiling";
            "configured" => configured,
            "ceiling" => ceiling,
            "effective" => effective,
        );
    }

    // 起動パス（process 内初回呼び出し）で effective 値を info ログに残す。
    // Once で一度だけ emit するため、毎サイクル出る warn と違い運用ノイズにならない。
    static STARTUP_LOG: std::sync::Once = std::sync::Once::new();
    STARTUP_LOG.call_once(|| {
        info!(log, "ref storage max top-up effective";
            "configured" => configured,
            "ceiling" => ceiling,
            "effective" => effective,
        );
    });

    NearToken::from_yoctonear(effective)
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
    mode: deposit::DepositMode,
) -> Result<C::Output> {
    let log = DEFAULT.new(o!("function" => "storage::deposit"));
    const METHOD_NAME: &str = "storage_deposit";
    let args = json!({
        "registration_only": mode.registration_only(),
    });
    let signer = wallet.signer();
    info!(log, "depositing";
        "value" => value.as_yoctonear(),
        "mode" => ?mode,
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
///
/// # Deployment Contract
///
/// This function serializes per-account calls via `REF_STORAGE_LOCKS`
/// (process-local `tokio::sync::Mutex`). Running multiple processes / containers
/// against the same `ROOT_ACCOUNT_ID` is forbidden (single-process invariant).
/// See `README.md#Deployment` for orchestrator-specific guards.
///
/// If the invariant is violated, the initial deposit may execute twice and the
/// per-call `max_top_up` cap degenerates to `max_top_up × concurrent_processes`.
/// Cross-process lock work is tracked in follow-up Issue #1 (pg_advisory_lock).
///
/// # Retry Contract
///
/// On `Err`, the caller MUST implement a retry ceiling / back-off. A
/// `register_tokens` rejection is recovered on the next cycle via a fresh
/// `balance_of` read; uncapped retry, however, accumulates up to
/// `max_top_up × retry_count` yoctoNEAR outside of the single-cycle cap
/// accounting. Caller-side back-off is tracked in follow-up Issue #2 and the
/// monitoring of cumulative top-up in follow-up Issue #3.
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
        // NOTE: この strict `>` が `remaining_cap = max_top_up.checked_sub(initial_deposit)`
        // (ステップ 5) の不変条件 `initial_deposit ≤ max_top_up` の根拠。緩和時は併せて検討。
        if amount > max_top_up {
            return Err(anyhow::anyhow!(
                "initial storage deposit {} yocto exceeds cap {} yocto",
                amount.as_yoctonear(),
                max_top_up.as_yoctonear(),
            ));
        }
        // `DepositMode::RegistrationOnly` で送金することで、REF Finance 側は必要量
        // （= `bounds.min`）ぴったりで登録し、超過分（= 0）のみを refund する。既に
        // 登録済みのアカウントだった場合は contract 仕様により全額 refund される
        // （contract_spec.md §2.2 参照）ため、stale view RPC での二重 deposit が起きても
        // storage_balance が過剰に増えない。
        //
        // 本コードは `amount == bounds.min` を前提に `RegistrationOnly` を選んでいる。
        // 将来、初期登録時に min_bound 以上を確保したくなった場合は
        // `DepositWithRegistration` に戻すか、refund ぶんを cap 会計から差し引く必要がある。
        deposit(
            client,
            wallet,
            amount,
            deposit::DepositMode::RegistrationOnly,
        )
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

    // planner の返す `Plan` variant で処理分岐する。
    //
    // `Plan::InitialRegister` は deposits 空で cap 検証を通らずに register_tokens を
    // 直接発行するパス。`register_tokens` の attached_deposit=1 yocto 前提により
    // storage 資金は動かず、`actual_top_up = 0` が型レベルで閉じ込められる。
    // `Plan::Normal` は unregister → cap 再評価 → top-up → register を通る標準パスで、
    // `remaining_cap` を計算する private helper に閉じ込めて cap-bypass の新経路が
    // 生まれないよう構造的に防御する。
    match planner::plan(&snapshot, needed_tokens, keep)? {
        planner::Plan::InitialRegister { to_register } => {
            handle_initial_register(client, wallet, &log, &to_register).await
        }
        planner::Plan::Normal {
            to_unregister,
            to_register,
            pre_unregister_estimate,
        } => {
            handle_normal_plan(
                client,
                wallet,
                &log,
                NormalPlanArgs {
                    account,
                    keep,
                    max_top_up,
                    initial_deposit,
                    to_unregister,
                    to_register,
                    pre_unregister_estimate,
                },
            )
            .await
        }
    }
}

/// `Plan::InitialRegister` を処理する。
///
/// deposits が空の初期登録ケース専用の経路。cap 検証は行わないため、このスコープ内で
/// `max_top_up` / `remaining_cap` / `actual_top_up` といった top-up 関連のシンボルには
/// 触れない。cap-bypass の安全前提（`register_tokens` が attached_deposit=1 yocto の
/// みで storage 資金を動かさない、初期 deposit の cap ガードは呼び出し元で完結済み）
/// が壊れないよう、責務をこの関数に閉じ込める。
async fn handle_initial_register<C, W>(
    client: &C,
    wallet: &W,
    log: &slog::Logger,
    to_register: &[TokenAccount],
) -> Result<()>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    debug!(log, "no existing deposits, registering tokens directly");
    if !to_register.is_empty() {
        deposit::register_tokens(client, wallet, to_register)
            .await?
            .wait_for_success()
            .await?;
        info!(log, "tokens registered"; "count" => to_register.len());
    }
    Ok(())
}

/// `handle_normal_plan` の非ジェネリック入力を束ねる private struct。
///
/// field 順は意味論グルーピング: (1) 呼び出しコンテキスト refs, (2) cap context,
/// (3) planner 出力 payload。named field 構築により位置 swap を型レベルで不能化。
#[derive(Debug)]
struct NormalPlanArgs<'a> {
    account: &'a AccountId,
    keep: &'a [TokenAccount],
    max_top_up: NearToken,
    initial_deposit: NearToken,
    to_unregister: Vec<TokenAccount>,
    to_register: Vec<TokenAccount>,
    pre_unregister_estimate: NearToken,
}

/// `Plan::Normal` を処理する。unregister → cap 再評価 → top-up → register。
///
/// cap 検証（ステップ 4-5）を必ず通してから `register_tokens` を発行する不変条件を
/// 関数境界で担保する。`pre_unregister_estimate` / `post_unregister_available` /
/// `actual_top_up` / `remaining_cap` はこの関数のローカルに閉じ込められる。
async fn handle_normal_plan<C, W>(
    client: &C,
    wallet: &W,
    log: &slog::Logger,
    args: NormalPlanArgs<'_>,
) -> Result<()>
where
    C: SendTx + ViewContract,
    W: Wallet,
{
    let NormalPlanArgs {
        account,
        keep,
        max_top_up,
        initial_deposit,
        to_unregister,
        to_register,
        pre_unregister_estimate,
    } = args;

    info!(log, "storage plan";
        "unregister" => to_unregister.len(),
        "register" => to_register.len(),
        "needed" => pre_unregister_estimate.as_yoctonear(),
    );

    // 3. ゼロ残高の旧トークンを unregister（TOCTOU 再検証 + チャンク分割）
    if !to_unregister.is_empty() {
        info!(log, "unregister stale tokens"; "count" => to_unregister.len());

        // TOCTOU ガード: 直前に再取得して amount == 0 && ∉ keep を再確認
        let fresh_deposits = deposit::get_deposits(client, account).await?;
        let verified: Vec<TokenAccount> = to_unregister
            .iter()
            .filter(|token| {
                fresh_deposits
                    .get(*token)
                    .is_some_and(|amount| amount.0 == 0)
                    && !keep.contains(token)
            })
            .cloned()
            .collect();

        let dropped = to_unregister.len() - verified.len();
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
    // `pre_unregister_estimate` は planner が初期 snapshot から推定した値
    // （`planner::Plan::Normal::pre_unregister_estimate` の doc 参照）で、unregister で
    // `deposits_len` が減った影響は反映されていない。saturating_sub は `available` 増加
    // のみ反映するため:
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
    let actual_top_up = pre_unregister_estimate
        .saturating_sub(NearToken::from_yoctonear(post_unregister_available));

    debug!(log, "top-up recalculated after unregister";
        "needed" => pre_unregister_estimate.as_yoctonear(),
        "post_unregister_available" => post_unregister_available,
        "actual_top_up" => actual_top_up.as_yoctonear(),
    );

    // 5. 累積支出が上限を超える場合はエラー
    //
    // 初期 deposit を実行した場合、そのぶんを max_top_up から差し引いた残り枠で
    // top-up の可否を判定する。これにより単一呼び出しでの総消費 NEAR が
    // max_top_up を超えないことを保証する（初期 deposit と top-up の二重キャップ回避）。
    //
    // 不変条件 `initial_deposit ≤ max_top_up` は step 1 の initial-deposit cap guard
    // (`amount > max_top_up → Err` の strict `>`) で保証される:
    //   - アカウント既登録時: `initial_deposit = 0 ≤ max_top_up`
    //   - アカウント未登録時: cap guard を通過するので `amount ≤ max_top_up`
    // この不変条件は `checked_sub.expect` で明示する。
    // saturating_sub は将来 step 1 が緩和された場合に silent 0-cap を許すため不採用。
    // 他 2 箇所の `saturating_sub` (storage.rs の post_unregister_available との差分、
    // planner.rs の `used.saturating_sub(min_bound)`) は意図的な saturate で別物。
    let remaining_cap = max_top_up
        .checked_sub(initial_deposit)
        .expect("initial_deposit ≤ max_top_up: enforced by initial-deposit cap guard in step 1");
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
        deposit(
            client,
            wallet,
            actual_top_up,
            deposit::DepositMode::DepositWithRegistration,
        )
        .await?
        .wait_for_success()
        .await?;
    }

    // 7. register_tokens
    if !to_register.is_empty() {
        info!(log, "registering tokens"; "count" => to_register.len());
        deposit::register_tokens(client, wallet, &to_register)
            .await?
            .wait_for_success()
            .await?;
        info!(log, "tokens registered"; "count" => to_register.len());
    }

    Ok(())
}

mod planner;

#[cfg(test)]
mod tests;
