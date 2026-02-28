use crate::Result;
use crate::recorder::TradeRecorder;
use bigdecimal::BigDecimal;
use common::config::ConfigAccess;
use common::types::{NearAmount, TokenInAccount, TokenOutAccount, YoctoAmount, YoctoValue};
use logging::*;
use near_sdk::{AccountId, NearToken};
use std::sync::atomic::{AtomicU64, Ordering};

// NOTE: LAST_HARVEST_TIME は cron 逐次実行のみからアクセスされるため Relaxed で十分。
// 並行化する場合は compare_exchange による排他制御が必要。
static LAST_HARVEST_TIME: AtomicU64 = AtomicU64::new(0);

fn harvest_interval(cfg: &impl ConfigAccess) -> u64 {
    cfg.harvest_interval_seconds()
}

fn harvest_account(cfg: &impl ConfigAccess) -> AccountId {
    let value = cfg.harvest_account_id().unwrap_or_else(|err| {
        let log = DEFAULT.new(o!("function" => "harvest_account"));
        warn!(log, "HARVEST_ACCOUNT_ID not set, using default";
            "error" => %err,
            "default" => "harvest.near"
        );
        "harvest.near".to_string()
    });
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse HARVEST_ACCOUNT_ID `{value}`: {err}"))
}

fn harvest_min_amount(cfg: &impl ConfigAccess) -> YoctoAmount {
    cfg.harvest_min_amount()
        .to_string()
        .parse::<NearAmount>()
        .unwrap()
        .to_yocto()
}

fn harvest_reserve_amount(cfg: &impl ConfigAccess) -> YoctoAmount {
    cfg.harvest_reserve_amount()
        .to_string()
        .parse::<NearAmount>()
        .unwrap()
        .to_yocto()
}

fn is_time_to_harvest(cfg: &impl ConfigAccess) -> bool {
    let last = LAST_HARVEST_TIME.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after UNIX epoch")
        .as_secs();
    now - last > harvest_interval(cfg)
}

fn update_last_harvest_time() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after UNIX epoch")
        .as_secs();
    LAST_HARVEST_TIME.store(now, Ordering::Relaxed);
}

/// ハーベスト判定と実行
///
/// 清算時に呼び出す。旧 period の initial_value と清算後の current_value を
/// 引数で受け取ることで、新 period 作成前に正しく判定できる。
///
/// # 引数
/// - `initial_value`: 旧 evaluation period の初期投資額
/// - `current_value`: 清算後のポートフォリオ価値
/// - `period_id`: ハーベスト取引を記録する evaluation period の ID
///
/// # 戻り値
/// 実際にハーベストした額（YoctoAmount）。ハーベストしなかった場合は zero。
pub async fn check_and_execute_harvest(
    initial_value: &YoctoValue,
    current_value: &YoctoValue,
    period_id: &str,
    cfg: &impl ConfigAccess,
) -> Result<YoctoAmount> {
    let log = DEFAULT.new(o!("function" => "check_and_execute_harvest"));
    check_and_execute_harvest_inner(initial_value, current_value, period_id, &log, cfg).await
}

/// ハーベスト判定と実行の内部ロジック
///
/// initial_value と current_value を直接受け取り、DB 依存なしで判定する。
async fn check_and_execute_harvest_inner(
    initial_value: &YoctoValue,
    current_value: &YoctoValue,
    period_id: &str,
    log: &slog::Logger,
    cfg: &impl ConfigAccess,
) -> Result<YoctoAmount> {
    debug!(log, "Portfolio value check for harvest";
        "initial_value" => %initial_value,
        "current_value" => %current_value
    );

    // initial_value がゼロの場合はハーベスト不可（初回起動時のバグ防止）
    if initial_value.is_zero() {
        trace!(log, "initial_value is zero, skipping harvest");
        return Ok(YoctoAmount::zero());
    }

    // 200%利益時の判定（初期投資額の2倍になった場合）
    // &YoctoValue * BigDecimal = YoctoValue
    let harvest_threshold = initial_value * BigDecimal::from(2);

    if *current_value > harvest_threshold {
        // YoctoValue - &YoctoValue = YoctoValue
        let excess = current_value - &harvest_threshold;
        info!(log, "Harvest threshold exceeded, executing harvest";
            "threshold" => %harvest_threshold,
            "excess" => %excess
        );

        // 10%の利益確定（余剰分の10%をハーベスト）
        // &YoctoValue * BigDecimal = YoctoValue
        let harvest_value = &excess * BigDecimal::new(1.into(), 1); // 10% = 0.1

        // 価値を送金数量に変換（NEAR は価値=数量）
        let harvest_amount = harvest_value.to_amount();

        let harvest_account = harvest_account(cfg);
        let min_harvest_amount = harvest_min_amount(cfg);

        // YoctoAmount < YoctoAmount
        if harvest_amount < min_harvest_amount {
            trace!(log, "Harvest amount below minimum threshold, skipping";
                "harvest_amount" => %harvest_amount,
                "min_amount" => %min_harvest_amount
            );
            return Ok(YoctoAmount::zero());
        }

        debug!(log, "Executing harvest";
            "amount" => %harvest_amount,
            "target_account" => %harvest_account,
            "harvest_value" => %harvest_value
        );

        // ハーベスト時間条件もチェック
        if !is_time_to_harvest(cfg) {
            trace!(log, "Harvest time interval not met, skipping";
                "last_harvest_interval_hours" => (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("system clock is after UNIX epoch")
                    .as_secs() - LAST_HARVEST_TIME.load(Ordering::Relaxed)) / 3600
            );
            return Ok(YoctoAmount::zero());
        }

        // 実際のハーベスト実行
        execute_harvest_transfer(
            &harvest_account,
            harvest_amount.clone(),
            period_id,
            log,
            cfg,
        )
        .await?;

        // ハーベスト実行時間を更新
        update_last_harvest_time();

        Ok(harvest_amount)
    } else {
        // YoctoValue / YoctoValue = BigDecimal（比率）
        // NOTE: initial_value は上方のゼロガードで非ゼロが保証されている
        let current_percentage = (current_value / initial_value) * BigDecimal::from(100);
        trace!(log, "Portfolio value below harvest threshold";
            "current_percentage" => %current_percentage
        );

        Ok(YoctoAmount::zero())
    }
}

/// ハーベスト送金の実行
async fn execute_harvest_transfer(
    target_account: &AccountId,
    harvest_amount: YoctoAmount,
    period_id: &str,
    log: &slog::Logger,
    cfg: &impl ConfigAccess,
) -> Result<()> {
    use blockchain::jsonrpc::{AccountInfo, SendTx, SentTx};
    use blockchain::ref_finance::{deposit, token_account::WNEAR_TOKEN};
    use blockchain::wallet::Wallet;

    debug!(log, "Starting harvest transfer execution";
        "target" => %target_account,
        "amount" => %harvest_amount
    );

    // YoctoAmount → u128 変換（ブロックチェーン API 境界）
    let harvest_amount_u128: u128 = harvest_amount.to_u128();

    // クライアントとウォレットの準備
    let client = blockchain::jsonrpc::new_client();
    let wallet = blockchain::wallet::new_wallet(cfg);

    trace!(log, "Executing harvest sequence";
        "step" => "1_withdraw_from_ref_finance",
        "amount" => %harvest_amount_u128
    );

    // 1. ref_finance depositからwrap.nearを引き出し
    let harvest_amount_token = NearToken::from_yoctonear(harvest_amount_u128);
    let withdraw_tx =
        deposit::withdraw(&client, &wallet, &WNEAR_TOKEN, harvest_amount_token).await?;

    let withdraw_result = withdraw_tx.wait_for_success().await;
    if let Err(e) = withdraw_result {
        error!(log, "Failed to withdraw from ref_finance";
            "error" => %e,
            "amount" => %harvest_amount_u128
        );
        return Err(anyhow::anyhow!("Harvest failed at withdrawal step: {}", e));
    }

    trace!(log, "Executing harvest sequence";
        "step" => "2_unwrap_to_native_near",
        "amount" => %harvest_amount_u128
    );

    // 2. wrap.nearをNEARに変換（unwrap）
    let unwrap_tx = deposit::wnear::unwrap(&client, &wallet, harvest_amount_token).await?;

    let unwrap_result = unwrap_tx.wait_for_success().await;
    if let Err(e) = unwrap_result {
        error!(log, "Failed to unwrap NEAR";
            "error" => %e,
            "amount" => %harvest_amount_u128
        );
        // unwrapに失敗した場合、wrap.nearをref_financeに戻すことを検討
        // ただし、ここでは単にエラーを返す
        return Err(anyhow::anyhow!("Harvest failed at unwrap step: {}", e));
    }

    trace!(log, "Executing harvest sequence";
        "step" => "3_transfer_to_target",
        "target" => %target_account,
        "amount" => %harvest_amount_u128
    );

    // 3. 保護額を考慮した送金額の計算
    let account_id = wallet.account_id();
    let current_native_balance = client.get_native_amount(account_id).await?;

    // 保護額をu128に変換
    let reserve_amount_u128: u128 = harvest_reserve_amount(cfg).to_u128();
    let reserve_amount_token = NearToken::from_yoctonear(reserve_amount_u128);

    // 送金可能額 = 現在残高 - 保護額
    let available_for_transfer = if current_native_balance > reserve_amount_token {
        current_native_balance.saturating_sub(reserve_amount_token)
    } else {
        trace!(log, "Insufficient balance for harvest transfer after reserve";
            "current_balance" => current_native_balance.as_yoctonear(),
            "reserve_amount" => reserve_amount_u128
        );
        return Ok(()); // 保護額を下回る場合は送金をスキップ
    };

    // 実際の送金額は予定額と送金可能額の小さい方
    let actual_transfer_amount = if harvest_amount_token < available_for_transfer {
        harvest_amount_token
    } else {
        available_for_transfer
    };

    trace!(log, "Executing harvest sequence";
        "step" => "3_transfer_to_target",
        "target" => %target_account,
        "planned_amount" => harvest_amount_u128,
        "available_for_transfer" => available_for_transfer.as_yoctonear(),
        "actual_transfer_amount" => actual_transfer_amount.as_yoctonear(),
        "current_native_balance" => current_native_balance.as_yoctonear(),
        "reserve_amount" => reserve_amount_u128
    );

    // 実際の送金実行
    let signer = wallet.signer();
    let sent_tx = client
        .transfer_native_token(signer, target_account, actual_transfer_amount)
        .await?;

    // トランザクションの完了を待つ
    let tx_outcome = sent_tx.wait_for_executed().await?;

    // トランザクションハッシュを取得
    let tx_hash = match tx_outcome {
        near_primitives::views::FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(view) => {
            view.transaction_outcome.id.to_string()
        }
        near_primitives::views::FinalExecutionOutcomeViewEnum::FinalExecutionOutcomeWithReceipt(
            view,
        ) => view.final_outcome.transaction_outcome.id.to_string(),
    };

    info!(log, "Harvest transfer completed successfully";
        "target" => %target_account,
        "planned_amount" => %harvest_amount_u128,
        "actual_amount" => %actual_transfer_amount,
        "tx_hash" => %tx_hash
    );

    // 4. ハーベスト取引をTradeTransactionに記録（実際の送金額で記録）
    // wNEAR → NEAR 変換なので、どちらも decimals=24
    let actual_transfer_yocto = YoctoAmount::from_u128(actual_transfer_amount.as_yoctonear());
    let from_amount = actual_transfer_yocto.to_token_amount();
    let to_amount = actual_transfer_yocto.to_token_amount();

    // 型安全なトークン型を使用
    use blockchain::ref_finance::token_account::NEAR_TOKEN;
    let from_token: TokenInAccount = WNEAR_TOKEN.to_in();
    let to_token: TokenOutAccount = NEAR_TOKEN.to_out();

    let recorder = TradeRecorder::new(period_id.to_string());
    recorder
        .record_trade(
            tx_hash, // 実際のトランザクションハッシュを使用
            &from_token,
            from_amount,
            &to_token,
            to_amount,
        )
        .await?;

    info!(log, "Harvest transaction recorded";
        "batch_id" => recorder.get_batch_id()
    );

    Ok(())
}

#[cfg(test)]
mod tests;
