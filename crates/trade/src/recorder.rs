use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use common::types::TokenAmount;
use common::types::{TokenInAccount, TokenOutAccount};
use logging::*;
use num_traits::Zero;
use uuid::Uuid;

use persistence::trade_transaction::TradeTransaction;

pub struct TradeRecorder {
    batch_id: String,
    evaluation_period_id: String,
}

impl TradeRecorder {
    pub fn new(evaluation_period_id: String) -> Self {
        let batch_id = Uuid::new_v4().to_string();
        let log = DEFAULT.new(o!("function" => "TradeRecorder::new"));
        trace!(log, "created new trade recorder";
            "batch_id" => %batch_id,
            "period_id" => %evaluation_period_id
        );
        Self {
            batch_id,
            evaluation_period_id,
        }
    }

    pub fn get_batch_id(&self) -> &str {
        &self.batch_id
    }

    pub async fn record_trade(
        &self,
        tx_id: String,
        from_token: &TokenInAccount,
        from_amount: TokenAmount,
        to_token: &TokenOutAccount,
        to_amount: TokenAmount,
        actual_to_amount: Option<TokenAmount>,
    ) -> Result<TradeTransaction> {
        let log = DEFAULT.new(o!("function" => "record_trade"));
        debug!(log, "recording trade"; "from_token" => %from_token, "to_token" => %to_token, "tx_id" => %tx_id);

        let from_smallest = from_amount.into_smallest_units();
        let to_smallest = to_amount.into_smallest_units();

        let actual_to_smallest: Option<BigDecimal> =
            actual_to_amount.map(|a| a.into_smallest_units().into());

        if let Some(ref actual_bd) = actual_to_smallest {
            let estimated_bd = to_smallest.as_bigdecimal();
            if estimated_bd.is_zero() {
                warn!(log, "skipping slippage calculation: estimated amount is zero";
                    "actual" => %actual_bd,
                    "to_token" => %to_token
                );
            } else {
                // diff_pct > 0: actual > estimated (有利な約定)
                // diff_pct < 0: actual < estimated (不利な約定 = スリッページ損)
                let diff = actual_bd - estimated_bd;
                let diff_pct = (&diff / estimated_bd * BigDecimal::from(100))
                    .with_scale_round(4, bigdecimal::RoundingMode::HalfUp);
                debug!(log, "swap slippage";
                    "estimated" => %estimated_bd,
                    "actual" => %actual_bd,
                    "diff_pct" => %diff_pct,
                    "to_token" => %to_token
                );
            }
        }

        debug!(log, "recording trade details";
            "from_amount" => %from_smallest,
            "from_token" => %from_token,
            "to_amount" => %to_smallest,
            "to_token" => %to_token,
            "batch_id" => %self.batch_id
        );

        let transaction = TradeTransaction {
            tx_id: tx_id.clone(),
            trade_batch_id: self.batch_id.clone(),
            from_token: from_token.to_string(),
            from_amount: from_smallest,
            to_token: to_token.to_string(),
            to_amount: to_smallest,
            timestamp: chrono::Utc::now().naive_utc(),
            evaluation_period_id: self.evaluation_period_id.clone(),
            actual_to_amount: actual_to_smallest,
        };

        let result = transaction
            .insert_async()
            .await
            .with_context(|| format!("Failed to insert trade transaction: {}", tx_id))?;

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use common::types::{TokenAccount, YoctoAmount};
    use futures::FutureExt;
    use std::panic::AssertUnwindSafe;

    // テスト用定数
    const WNEAR_DECIMALS: u8 = 24;
    const TEST_TOKEN_DECIMALS: u8 = 18;

    /// テスト用の TokenAmount を作成（smallest_units の u128 値と decimals を指定）
    fn token_amount(smallest_units: u128, decimals: u8) -> TokenAmount {
        TokenAmount::from_smallest_units(BigDecimal::from(smallest_units), decimals)
    }

    #[tokio::test]
    async fn test_trade_recorder() {
        use persistence::evaluation_period::NewEvaluationPeriod;

        // 評価期間を作成（外部キー制約のため）
        // 初期投資額: 100 NEAR (= 100e24 yocto)
        let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000); // 100 NEAR
        let new_period = NewEvaluationPeriod::new(initial_value, vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;

        let recorder = TradeRecorder::new(period_id.clone());
        let batch_id = recorder.get_batch_id().to_string();

        let tx_id = format!("test_tx_{}", Uuid::new_v4());
        let from_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
        let to_token: TokenOutAccount = "akaia.tkn.near".parse::<TokenAccount>().unwrap().into();

        let result = AssertUnwindSafe(async {
            let result = recorder
                .record_trade(
                    tx_id.clone(),
                    &from_token,
                    token_amount(1_000_000_000_000_000_000_000_000, WNEAR_DECIMALS), // 1 wNEAR
                    &to_token,
                    token_amount(50_000_000_000_000_000_000_000, TEST_TOKEN_DECIMALS), // 50000 tokens (estimated)
                    Some(token_amount(
                        49_500_000_000_000_000_000_000,
                        TEST_TOKEN_DECIMALS,
                    )), // actual
                )
                .await
                .unwrap();

            assert_eq!(result.tx_id, tx_id);
            assert_eq!(result.trade_batch_id, batch_id);
            // actual_to_amount が DB に正しく保存されていることを検証
            let found = persistence::trade_transaction::TradeTransaction::find_by_tx_id_async(
                tx_id.clone(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(found.actual_to_amount.is_some());
            assert_eq!(
                found.actual_to_amount.unwrap(),
                BigDecimal::from(49_500_000_000_000_000_000_000_u128)
            );
        })
        .catch_unwind()
        .await;

        // Cleanup（テスト本体がパニックしても常に実行）
        let _ =
            persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id).await;
        let _ =
            persistence::evaluation_period::EvaluationPeriod::delete_by_period_id_async(period_id)
                .await;

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }

    #[tokio::test]
    async fn test_trade_recorder_without_actual_amount() {
        use persistence::evaluation_period::NewEvaluationPeriod;

        let initial_value = YoctoAmount::from_u128(100_000_000_000_000_000_000_000_000);
        let new_period = NewEvaluationPeriod::new(initial_value, vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;

        let recorder = TradeRecorder::new(period_id.clone());

        let tx_id = format!("test_tx_{}", Uuid::new_v4());
        let from_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
        let to_token: TokenOutAccount = "akaia.tkn.near".parse::<TokenAccount>().unwrap().into();

        let result = AssertUnwindSafe(async {
            let result = recorder
                .record_trade(
                    tx_id.clone(),
                    &from_token,
                    token_amount(1_000_000_000_000_000_000_000_000, WNEAR_DECIMALS),
                    &to_token,
                    token_amount(50_000_000_000_000_000_000_000, TEST_TOKEN_DECIMALS),
                    None,
                )
                .await
                .unwrap();

            // actual_to_amount が NULL として保存されていることを検証
            let found = persistence::trade_transaction::TradeTransaction::find_by_tx_id_async(
                tx_id.clone(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(found.actual_to_amount.is_none());
            assert_eq!(result.tx_id, tx_id);
        })
        .catch_unwind()
        .await;

        // Cleanup（テスト本体がパニックしても常に実行）
        let _ =
            persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id).await;
        let _ =
            persistence::evaluation_period::EvaluationPeriod::delete_by_period_id_async(period_id)
                .await;

        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
