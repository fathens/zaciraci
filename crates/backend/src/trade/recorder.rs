use crate::logging::*;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use anyhow::{Context, Result};
use uuid::Uuid;
use zaciraci_common::types::TokenAmount;

use crate::persistence::trade_transaction::TradeTransaction;

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
    ) -> Result<TradeTransaction> {
        let log = DEFAULT.new(o!("function" => "record_trade"));
        debug!(log, "recording trade"; "from_token" => %from_token, "to_token" => %to_token, "tx_id" => %tx_id);

        // 型安全な値をDB層用のBigDecimalに変換
        let from_amount_bd = from_amount.smallest_units().clone();
        let to_amount_bd = to_amount.smallest_units().clone();

        let transaction = TradeTransaction {
            tx_id: tx_id.clone(),
            trade_batch_id: self.batch_id.clone(),
            from_token: from_token.to_string(),
            from_amount: from_amount_bd.clone(),
            to_token: to_token.to_string(),
            to_amount: to_amount_bd.clone(),
            timestamp: chrono::Utc::now().naive_utc(),
            evaluation_period_id: Some(self.evaluation_period_id.clone()),
        };

        let result = transaction
            .insert_async()
            .await
            .with_context(|| format!("Failed to insert trade transaction: {}", tx_id))?;

        debug!(log, "successfully recorded trade";
            "from_amount" => %from_amount_bd,
            "from_token" => %from_token,
            "to_amount" => %to_amount_bd,
            "to_token" => %to_token,
            "batch_id" => %self.batch_id
        );

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::TokenAccount;
    use bigdecimal::BigDecimal;

    // テスト用定数
    const WNEAR_DECIMALS: u8 = 24;
    const TEST_TOKEN_DECIMALS: u8 = 18;

    /// テスト用の TokenAmount を作成（smallest_units の u128 値と decimals を指定）
    fn token_amount(smallest_units: u128, decimals: u8) -> TokenAmount {
        TokenAmount::from_smallest_units(BigDecimal::from(smallest_units), decimals)
    }

    #[tokio::test]
    async fn test_trade_recorder() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;

        // 評価期間を作成（外部キー制約のため）
        // 初期投資額: 100 NEAR (= 100e24 yocto)
        let initial_value = BigDecimal::from(100_000_000_000_000_000_000_000_000u128); // 100 NEAR
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
                token_amount(50_000_000_000_000_000_000_000, TEST_TOKEN_DECIMALS), // 50000 tokens
            )
            .await
            .unwrap();

        assert_eq!(result.tx_id, tx_id);
        assert_eq!(result.trade_batch_id, batch_id);

        // Cleanup
        crate::persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }
}
