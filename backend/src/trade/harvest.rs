use crate::Result;
use crate::config;
use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::trade::recorder::TradeRecorder;
use crate::wallet::Wallet;
use bigdecimal::BigDecimal;
use near_sdk::AccountId;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};

// ハーベスト関連の定数とstatic変数
const HARVEST_INTERVAL: u64 = 24 * 60 * 60; // 24時間
static LAST_HARVEST_TIME: AtomicU64 = AtomicU64::new(0);
static HARVEST_ACCOUNT: Lazy<AccountId> = Lazy::new(|| {
    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|err| {
        eprintln!(
            "Warning: HARVEST_ACCOUNT_ID not set, using default. Error: {}",
            err
        );
        "harvest.near".to_string()
    });
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse HARVEST_ACCOUNT_ID `{}`: {}", value, err))
});
static HARVEST_MIN_AMOUNT: Lazy<BigDecimal> = Lazy::new(|| {
    let min_str = config::get("HARVEST_MIN_AMOUNT").unwrap_or_else(|_| "10".to_string());
    let min_near = min_str.parse::<u64>().unwrap_or(10);
    BigDecimal::from(min_near) * BigDecimal::from(1_000_000_000_000_000_000_000_000u128) // yoctoNEAR変換
});

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST_TIME.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > HARVEST_INTERVAL
}

fn update_last_harvest_time() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST_TIME.store(now, Ordering::Relaxed);
}

/// ハーベスト判定と実行
pub async fn check_and_harvest(initial_amount: u128) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "check_and_harvest"));

    // 最新のバッチIDを取得してポートフォリオ価値を計算
    let latest_batch_id =
        match crate::persistence::trade_transaction::TradeTransaction::get_latest_batch_id_async()
            .await?
        {
            Some(batch_id) => batch_id,
            None => {
                debug!(log, "No trades recorded yet, skipping harvest");
                return Ok(());
            }
        };

    let current_portfolio_value = crate::persistence::trade_transaction::TradeTransaction::get_portfolio_value_by_batch_async(latest_batch_id.clone()).await?;
    let initial_value = BigDecimal::from(initial_amount);

    info!(log, "Portfolio value check";
        "initial_value" => %initial_value,
        "current_value" => %current_portfolio_value,
        "batch_id" => %latest_batch_id
    );

    // 200%利益時の判定（初期投資額の3倍になった場合）
    let harvest_threshold = &initial_value * BigDecimal::from(3); // 300% = 3倍

    if current_portfolio_value > harvest_threshold {
        info!(log, "Harvest threshold exceeded, executing harvest";
            "threshold" => %harvest_threshold,
            "excess" => %(&current_portfolio_value - &harvest_threshold)
        );

        // 10%の利益確定（余剰分の10%をハーベスト）
        let excess_value = &current_portfolio_value - &harvest_threshold;
        let harvest_amount = &excess_value * BigDecimal::new(1.into(), 1); // 10% = 0.1

        // static変数から設定値を取得
        let harvest_account = &*HARVEST_ACCOUNT;
        let min_harvest_amount = &*HARVEST_MIN_AMOUNT;

        if harvest_amount < *min_harvest_amount {
            info!(log, "Harvest amount below minimum threshold, skipping";
                "harvest_amount" => %harvest_amount,
                "min_amount" => %min_harvest_amount
            );
            return Ok(());
        }

        info!(log, "Executing harvest";
            "amount" => %harvest_amount,
            "target_account" => %harvest_account,
            "excess_value" => %excess_value
        );

        // ハーベスト時間条件もチェック
        if !is_time_to_harvest() {
            info!(log, "Harvest time interval not met, skipping";
                "last_harvest_interval_hours" => (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() - LAST_HARVEST_TIME.load(Ordering::Relaxed)) / 3600
            );
            return Ok(());
        }

        // 実際のハーベスト実行
        execute_harvest_transfer(harvest_account, harvest_amount, &log).await?;

        // ハーベスト実行時間を更新
        update_last_harvest_time();
    } else {
        debug!(log, "Portfolio value below harvest threshold";
            "current_percentage" => %((&current_portfolio_value / &initial_value) * BigDecimal::from(100))
        );
    }

    Ok(())
}

/// ハーベスト送金の実行
async fn execute_harvest_transfer(
    target_account: &AccountId,
    harvest_amount: BigDecimal,
    log: &slog::Logger,
) -> Result<()> {
    use crate::jsonrpc::SendTx;
    use crate::ref_finance::{deposit, token_account::WNEAR_TOKEN};

    info!(log, "Starting harvest transfer execution";
        "target" => %target_account,
        "amount" => %harvest_amount
    );

    // BigDecimal → u128 変換（yoctoNEAR）
    let harvest_amount_u128: u128 = harvest_amount
        .to_string()
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to convert harvest amount to u128: {}", e))?;

    // クライアントとウォレットの準備
    let client = crate::jsonrpc::new_client();
    let wallet = crate::wallet::new_wallet();

    info!(log, "Executing harvest sequence";
        "step" => "1_withdraw_from_ref_finance",
        "amount" => %harvest_amount_u128
    );

    // 1. ref_finance depositからwrap.nearを引き出し
    deposit::withdraw(&client, &wallet, &WNEAR_TOKEN, harvest_amount_u128)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "Executing harvest sequence";
        "step" => "2_unwrap_to_native_near",
        "amount" => %harvest_amount_u128
    );

    // 2. wrap.nearをNEARに変換（unwrap）
    deposit::wnear::unwrap(&client, &wallet, harvest_amount_u128)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "Executing harvest sequence";
        "step" => "3_transfer_to_target",
        "target" => %target_account,
        "amount" => %harvest_amount_u128
    );

    // 3. 指定アカウントに送金
    let signer = wallet.signer();
    client
        .transfer_native_token(signer, target_account, harvest_amount_u128)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "Harvest transfer completed successfully";
        "target" => %target_account,
        "amount" => %harvest_amount_u128
    );

    // 4. ハーベスト取引をTradeTransactionに記録
    let recorder = TradeRecorder::new();
    recorder
        .record_trade(
            "harvest_tx_placeholder".to_string(), // 実際にはトランザクションハッシュを使用
            "wrap.near".to_string(),
            harvest_amount.clone(),
            "near".to_string(),
            harvest_amount.clone(),
            harvest_amount.clone(), // yoctoNEAR建て価格
        )
        .await?;

    info!(log, "Harvest transaction recorded";
        "batch_id" => recorder.get_batch_id()
    );

    Ok(())
}
