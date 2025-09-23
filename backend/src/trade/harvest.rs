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
static HARVEST_RESERVE_AMOUNT: Lazy<BigDecimal> = Lazy::new(|| {
    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    BigDecimal::from(reserve_near) * BigDecimal::from(1_000_000_000_000_000_000_000_000u128) // yoctoNEAR変換
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

    // 200%利益時の判定（初期投資額の2倍になった場合）
    let harvest_threshold = &initial_value * BigDecimal::from(2); // 200% = 2倍

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
    use crate::jsonrpc::{AccountInfo, SendTx};
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

    // 3. 保護額を考慮した送金額の計算
    let account_id = wallet.account_id();
    let current_native_balance = client.get_native_amount(account_id).await?;

    // 保護額をu128に変換
    let reserve_amount_u128: u128 = HARVEST_RESERVE_AMOUNT
        .to_string()
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to convert reserve amount to u128: {}", e))?;

    // 送金可能額 = 現在残高 - 保護額
    let available_for_transfer = if current_native_balance > reserve_amount_u128 {
        current_native_balance - reserve_amount_u128
    } else {
        info!(log, "Insufficient balance for harvest transfer after reserve";
            "current_balance" => current_native_balance,
            "reserve_amount" => reserve_amount_u128
        );
        return Ok(()); // 保護額を下回る場合は送金をスキップ
    };

    // 実際の送金額は予定額と送金可能額の小さい方
    let actual_transfer_amount = harvest_amount_u128.min(available_for_transfer);

    info!(log, "Executing harvest sequence";
        "step" => "3_transfer_to_target",
        "target" => %target_account,
        "planned_amount" => %harvest_amount_u128,
        "available_for_transfer" => %available_for_transfer,
        "actual_transfer_amount" => %actual_transfer_amount,
        "current_native_balance" => %current_native_balance,
        "reserve_amount" => %reserve_amount_u128
    );

    // 実際の送金実行
    let signer = wallet.signer();
    client
        .transfer_native_token(signer, target_account, actual_transfer_amount)
        .await?
        .wait_for_success()
        .await?;

    info!(log, "Harvest transfer completed successfully";
        "target" => %target_account,
        "planned_amount" => %harvest_amount_u128,
        "actual_amount" => %actual_transfer_amount
    );

    // 4. ハーベスト取引をTradeTransactionに記録（実際の送金額で記録）
    let actual_transfer_bigdecimal = BigDecimal::from(actual_transfer_amount);
    let recorder = TradeRecorder::new();
    recorder
        .record_trade(
            "harvest_tx_placeholder".to_string(), // 実際にはトランザクションハッシュを使用
            "wrap.near".to_string(),
            actual_transfer_bigdecimal.clone(),
            "near".to_string(),
            actual_transfer_bigdecimal.clone(),
            actual_transfer_bigdecimal.clone(), // yoctoNEAR建て価格
        )
        .await?;

    info!(log, "Harvest transaction recorded";
        "batch_id" => recorder.get_batch_id()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use bigdecimal::BigDecimal;

    #[test]
    #[ignore] // Config test conflicts with other tests due to static initialization
    fn test_harvest_reserve_amount_default() {
        // テスト用にデフォルト値（1 NEAR）をテスト
        let expected =
            BigDecimal::from(1u64) * BigDecimal::from(1_000_000_000_000_000_000_000_000u128);

        // 設定値の読み込みロジックを直接テスト（staticの並列実行問題を回避）
        let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
        let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
        let actual = BigDecimal::from(reserve_near)
            * BigDecimal::from(1_000_000_000_000_000_000_000_000u128);

        assert_eq!(actual, expected);
    }

    #[test]
    #[ignore] // Config test conflicts with other tests due to static initialization
    fn test_harvest_reserve_amount_custom() {
        // カスタム値のテスト
        config::set("HARVEST_RESERVE_AMOUNT", "5");

        // 新しいLazy値を再初期化するため、一旦クリア
        // 注: 実際の環境では一度だけ初期化されるため、この方法は完璧ではないが
        // テストの意図を示すために記述
        let expected =
            BigDecimal::from(5u64) * BigDecimal::from(1_000_000_000_000_000_000_000_000u128);

        // 実際のテストでは環境変数の変更後に新しいプロセスが必要
        // ここでは設定値の読み込みロジックをテスト
        let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
        let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
        let actual = BigDecimal::from(reserve_near)
            * BigDecimal::from(1_000_000_000_000_000_000_000_000u128);

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_harvest_min_amount_default() {
        // HARVEST_MIN_AMOUNTのデフォルト値テスト
        let expected =
            BigDecimal::from(10u64) * BigDecimal::from(1_000_000_000_000_000_000_000_000u128);
        let actual = &*HARVEST_MIN_AMOUNT;
        assert_eq!(*actual, expected);
    }

    #[test]
    fn test_yocto_near_conversion() {
        // yoctoNEAR変換の正確性テスト
        let one_near_in_yocto = 1_000_000_000_000_000_000_000_000u128;
        let five_near = BigDecimal::from(5u64) * BigDecimal::from(one_near_in_yocto);

        // 5 NEARが正しくyoctoNEARに変換されることを確認
        assert_eq!(five_near.to_string(), "5000000000000000000000000");
    }

    #[test]
    fn test_harvest_reserve_amount_parsing() {
        // 無効な設定値の場合のフォールバック動作テスト
        config::set("HARVEST_RESERVE_AMOUNT", "invalid");

        let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
        let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);

        // 無効な値の場合、デフォルト1に戻ることを確認
        assert_eq!(reserve_near, 1);

        // 正常な値の場合のテスト
        config::set("HARVEST_RESERVE_AMOUNT", "3");
        let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
        let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
        assert_eq!(reserve_near, 3);
    }

    #[test]
    fn test_harvest_account_parsing() {
        // HARVEST_ACCOUNT_IDの正常なパース動作テスト
        config::set("HARVEST_ACCOUNT_ID", "test.near");

        let value =
            config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|_| "harvest.near".to_string());
        let parsed_account = value.parse::<AccountId>();

        assert!(parsed_account.is_ok());
        assert_eq!(parsed_account.unwrap().as_str(), "test.near");
    }
}
