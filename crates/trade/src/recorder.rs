use anyhow::{Context, Result};
use bigdecimal::{BigDecimal, Zero};
use common::types::TokenAmount;
use common::types::{TokenInAccount, TokenOutAccount};
use logging::*;
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
        let to_smallest = to_amount.clone().into_smallest_units();

        if let Some(ref actual) = actual_to_amount {
            let estimated_whole = to_amount.to_whole();
            let actual_whole = actual.to_whole();
            let diff_pct = if !estimated_whole.is_zero() {
                let diff = &actual_whole - &estimated_whole;
                (&diff / &estimated_whole) * BigDecimal::from(100)
            } else {
                BigDecimal::from(0)
            };
            info!(log, "swap slippage";
                "estimated" => %estimated_whole,
                "actual" => %actual_whole,
                "diff_pct" => %diff_pct,
                "to_token" => %to_token
            );
        }

        let actual_to_smallest: Option<BigDecimal> =
            actual_to_amount.map(|a| a.into_smallest_units().into());

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
            evaluation_period_id: Some(self.evaluation_period_id.clone()),
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

        let recorder = TradeRecorder::new(period_id);
        let batch_id = recorder.get_batch_id().to_string();

        let tx_id = format!("test_tx_{}", Uuid::new_v4());
        let from_token: TokenInAccount = "wrap.near".parse::<TokenAccount>().unwrap().into();
        let to_token: TokenOutAccount = "akaia.tkn.near".parse::<TokenAccount>().unwrap().into();
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

        // Cleanup
        persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }
}
