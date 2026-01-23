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
        info!(log, "created new trade recorder";
            "batch_id" => %batch_id,
            "period_id" => %evaluation_period_id
        );
        Self {
            batch_id,
            evaluation_period_id,
        }
    }

    #[allow(dead_code)]
    pub fn with_batch_id(batch_id: String, evaluation_period_id: String) -> Self {
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

        let transaction = TradeTransaction::new(
            tx_id.clone(),
            self.batch_id.clone(),
            from_token.to_string(),
            from_amount_bd.clone(),
            to_token.to_string(),
            to_amount_bd.clone(),
            Some(self.evaluation_period_id.clone()),
        );

        let result = transaction
            .insert_async()
            .await
            .with_context(|| format!("Failed to insert trade transaction: {}", tx_id))?;

        info!(log, "successfully recorded trade";
            "from_amount" => %from_amount_bd,
            "from_token" => %from_token,
            "to_amount" => %to_amount_bd,
            "to_token" => %to_token,
            "batch_id" => %self.batch_id
        );

        Ok(result)
    }

    #[allow(dead_code)] // テスト専用
    pub async fn record_batch(&self, trades: Vec<TradeData>) -> Result<Vec<TradeTransaction>> {
        let log = DEFAULT.new(o!("function" => "record_batch"));
        info!(log, "recording batch of trades";
            "count" => trades.len(),
            "batch_id" => %self.batch_id
        );

        let transactions: Vec<TradeTransaction> = trades
            .into_iter()
            .map(|trade| {
                TradeTransaction::new(
                    trade.tx_id,
                    self.batch_id.clone(),
                    trade.from_token.to_string(),
                    trade.from_amount.smallest_units().clone(),
                    trade.to_token.to_string(),
                    trade.to_amount.smallest_units().clone(),
                    Some(self.evaluation_period_id.clone()),
                )
            })
            .collect();

        let results = TradeTransaction::insert_batch_async(transactions)
            .await
            .context("Failed to insert batch of trade transactions")?;

        info!(log, "successfully recorded trades in batch";
            "count" => results.len(),
            "batch_id" => %self.batch_id
        );

        Ok(results)
    }

    #[allow(dead_code)]
    pub async fn get_batch_transactions(&self) -> Result<Vec<TradeTransaction>> {
        TradeTransaction::find_by_batch_id_async(self.batch_id.clone())
            .await
            .context("Failed to get batch transactions")
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // テスト専用
pub struct TradeData {
    pub tx_id: String,
    pub from_token: TokenInAccount,
    pub from_amount: TokenAmount,
    pub to_token: TokenOutAccount,
    pub to_amount: TokenAmount,
}

impl TradeData {
    #[allow(dead_code)] // テスト専用
    pub fn new(
        tx_id: String,
        from_token: TokenInAccount,
        from_amount: TokenAmount,
        to_token: TokenOutAccount,
        to_amount: TokenAmount,
    ) -> Self {
        Self {
            tx_id,
            from_token,
            from_amount,
            to_token,
            to_amount,
        }
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

    #[tokio::test]
    async fn test_batch_recording() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;

        // 評価期間を作成（外部キー制約のため）
        // 初期投資額: 100 NEAR (= 100e24 yocto)
        let initial_value = BigDecimal::from(100_000_000_000_000_000_000_000_000u128); // 100 NEAR
        let new_period = NewEvaluationPeriod::new(initial_value, vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;

        let recorder = TradeRecorder::new(period_id);

        let trades = vec![
            TradeData::new(
                format!("test_tx1_{}", Uuid::new_v4()),
                "wrap.near".parse::<TokenAccount>().unwrap().into(),
                token_amount(1_000_000_000_000_000_000_000_000, WNEAR_DECIMALS), // 1 wNEAR
                "token1.near".parse::<TokenAccount>().unwrap().into(),
                token_amount(50_000_000_000_000_000_000_000, TEST_TOKEN_DECIMALS), // 50000 tokens
            ),
            TradeData::new(
                format!("test_tx2_{}", Uuid::new_v4()),
                "wrap.near".parse::<TokenAccount>().unwrap().into(),
                token_amount(2_000_000_000_000_000_000_000_000, WNEAR_DECIMALS), // 2 wNEAR
                "token2.near".parse::<TokenAccount>().unwrap().into(),
                token_amount(100_000_000_000_000_000_000_000, TEST_TOKEN_DECIMALS), // 100000 tokens
            ),
        ];

        let tx_ids: Vec<String> = trades.iter().map(|t| t.tx_id.clone()).collect();
        let results = recorder.record_batch(trades).await.unwrap();

        assert_eq!(results.len(), 2);

        // Cleanup
        for tx_id in tx_ids {
            crate::persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id)
                .await
                .unwrap();
        }
    }
}
