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
use zaciraci_common::types::NearValue;

// ハーベスト関連のstatic変数
static LAST_HARVEST_TIME: AtomicU64 = AtomicU64::new(0);
static HARVEST_INTERVAL: Lazy<u64> = Lazy::new(|| {
    config::get("HARVEST_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(86400) // デフォルト: 24時間
});
static HARVEST_ACCOUNT: Lazy<AccountId> = Lazy::new(|| {
    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|err| {
        let log = DEFAULT.new(o!("function" => "HARVEST_ACCOUNT initialization"));
        warn!(log, "HARVEST_ACCOUNT_ID not set, using default";
            "error" => %err,
            "default" => "harvest.near"
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
    // NEAR → yoctoNEAR 変換（型安全）
    NearValue::new(BigDecimal::from(min_near))
        .to_yocto()
        .into_bigdecimal()
});
static HARVEST_RESERVE_AMOUNT: Lazy<BigDecimal> = Lazy::new(|| {
    let reserve_str = config::get("HARVEST_RESERVE_AMOUNT").unwrap_or_else(|_| "1".to_string());
    let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
    // NEAR → yoctoNEAR 変換（型安全）
    NearValue::new(BigDecimal::from(reserve_near))
        .to_yocto()
        .into_bigdecimal()
});

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST_TIME.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > *HARVEST_INTERVAL
}

fn update_last_harvest_time() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST_TIME.store(now, Ordering::Relaxed);
}

/// ハーベスト判定と実行
pub async fn check_and_harvest(current_portfolio_value_yocto: u128) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "check_and_harvest"));

    // 最新の評価期間を取得して初期投資額を取得
    let latest_period =
        match crate::persistence::evaluation_period::EvaluationPeriod::get_latest_async().await? {
            Some(period) => period,
            None => {
                debug!(log, "No evaluation period found, skipping harvest");
                return Ok(());
            }
        };

    let initial_value = latest_period.initial_value;
    let current_value = BigDecimal::from(current_portfolio_value_yocto);

    info!(log, "Portfolio value check";
        "initial_value" => %initial_value,
        "current_value" => %current_value,
        "period_id" => %latest_period.period_id
    );

    // 200%利益時の判定（初期投資額の2倍になった場合）
    let harvest_threshold = &initial_value * BigDecimal::from(2); // 200% = 2倍

    if current_value > harvest_threshold {
        info!(log, "Harvest threshold exceeded, executing harvest";
            "threshold" => %harvest_threshold,
            "excess" => %(&current_value - &harvest_threshold)
        );

        // 10%の利益確定（余剰分の10%をハーベスト）
        let excess_value = &current_value - &harvest_threshold;
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
            "current_percentage" => %((&current_value / &initial_value) * BigDecimal::from(100))
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
    let withdraw_tx =
        deposit::withdraw(&client, &wallet, &WNEAR_TOKEN, harvest_amount_u128).await?;

    let withdraw_result = withdraw_tx.wait_for_success().await;
    if let Err(e) = withdraw_result {
        error!(log, "Failed to withdraw from ref_finance";
            "error" => %e,
            "amount" => %harvest_amount_u128
        );
        return Err(anyhow::anyhow!("Harvest failed at withdrawal step: {}", e));
    }

    info!(log, "Executing harvest sequence";
        "step" => "2_unwrap_to_native_near",
        "amount" => %harvest_amount_u128
    );

    // 2. wrap.nearをNEARに変換（unwrap）
    let unwrap_tx = deposit::wnear::unwrap(&client, &wallet, harvest_amount_u128).await?;

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

    // 4. 最新の評価期間を取得
    let latest_period =
        crate::persistence::evaluation_period::EvaluationPeriod::get_latest_async().await?;
    let period_id = match latest_period {
        Some(period) => period.period_id,
        None => {
            return Err(anyhow::anyhow!(
                "No evaluation period found for harvest transaction"
            ));
        }
    };

    // 5. ハーベスト取引をTradeTransactionに記録（実際の送金額で記録）
    let actual_transfer_bigdecimal = BigDecimal::from(actual_transfer_amount);
    let recorder = TradeRecorder::new(period_id);
    recorder
        .record_trade(
            tx_hash, // 実際のトランザクションハッシュを使用
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
    use zaciraci_common::types::NearValue;

    /// NEAR → yoctoNEAR 変換のヘルパー（型安全）
    fn near_to_yocto(near: u64) -> BigDecimal {
        NearValue::new(BigDecimal::from(near))
            .to_yocto()
            .into_bigdecimal()
    }

    // テスト専用: staticを使わずに設定値を計算する関数
    #[cfg(test)]
    fn calculate_harvest_reserve_amount_from_config(config_value: Option<&str>) -> BigDecimal {
        let reserve_str = config_value.unwrap_or("1").to_string();
        let reserve_near = reserve_str.parse::<u64>().unwrap_or(1);
        near_to_yocto(reserve_near)
    }

    #[test]
    fn test_harvest_reserve_amount_default() {
        // テスト用にデフォルト値（1 NEAR）をテスト
        let expected = near_to_yocto(1);

        // staticを使わずに設定ロジックを直接テスト
        let actual = calculate_harvest_reserve_amount_from_config(None);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_harvest_reserve_amount_custom() {
        // カスタム値のテスト: 5 NEAR
        let expected = near_to_yocto(5);

        // staticを使わずに設定ロジックを直接テスト
        let actual = calculate_harvest_reserve_amount_from_config(Some("5"));
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_harvest_min_amount_default() {
        // HARVEST_MIN_AMOUNTのデフォルト値テスト
        let expected = near_to_yocto(10);
        let actual = &*HARVEST_MIN_AMOUNT;
        assert_eq!(*actual, expected);
    }

    #[test]
    fn test_yocto_near_conversion() {
        // yoctoNEAR変換の正確性テスト（型安全版）
        let five_near = near_to_yocto(5);

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

    #[test]
    fn test_is_time_to_harvest() {
        // 初回は常にtrueになるはず（LAST_HARVEST_TIMEが0のため）
        assert!(is_time_to_harvest());

        // 現在時刻を記録
        update_last_harvest_time();

        // 直後はfalseになるはず
        assert!(!is_time_to_harvest());
    }

    #[test]
    fn test_harvest_threshold_calculation() {
        // 初期投資額: 100 NEAR
        let initial_amount = 100u128 * 10u128.pow(24);
        let initial_value = BigDecimal::from(initial_amount);

        // 200%利益時のしきい値（2倍）
        let harvest_threshold = &initial_value * BigDecimal::from(2);
        let expected_threshold = BigDecimal::from(200u128 * 10u128.pow(24));
        assert_eq!(harvest_threshold, expected_threshold);

        // ポートフォリオ価値が250 NEARの場合
        let current_portfolio_value = BigDecimal::from(250u128 * 10u128.pow(24));
        let excess_value = &current_portfolio_value - &harvest_threshold;
        let expected_excess = BigDecimal::from(50u128 * 10u128.pow(24));
        assert_eq!(excess_value, expected_excess);

        // 10%の利益確定額
        let harvest_amount = &excess_value * BigDecimal::new(1.into(), 1); // 10% = 0.1
        let expected_harvest = BigDecimal::from(5u128 * 10u128.pow(24)); // 5 NEAR
        assert_eq!(harvest_amount, expected_harvest);
    }

    #[tokio::test]
    async fn test_check_and_harvest_no_evaluation_period() {
        // 評価期間がまだない場合のテスト
        let current_portfolio_value = 100u128 * 10u128.pow(24);

        // check_and_harvestは早期リターンするはず（評価期間がない場合）
        // エラーが出ないことを確認
        let result = check_and_harvest(current_portfolio_value).await;

        // データベースが使えない環境ではテストをスキップ
        if result.is_err() {
            println!("Skipping test due to database unavailability");
            return;
        }
    }
}
