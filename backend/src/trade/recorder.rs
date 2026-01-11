use crate::logging::*;
use anyhow::{Context, Result};
use bigdecimal::BigDecimal;
use uuid::Uuid;
use zaciraci_common::types::{YoctoAmount, YoctoValue};

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
        from_token: String,
        from_amount: YoctoAmount,
        to_token: String,
        to_amount: YoctoAmount,
        price_yocto_near: YoctoValue,
    ) -> Result<TradeTransaction> {
        let log = DEFAULT.new(o!("function" => "record_trade"));
        debug!(log, "recording trade"; "from_token" => %from_token, "to_token" => %to_token, "tx_id" => %tx_id);

        // 型安全な値をDB層用のBigDecimalに変換
        let from_amount_bd = from_amount.clone().into_bigdecimal();
        let to_amount_bd = to_amount.clone().into_bigdecimal();
        let price_bd = price_yocto_near.clone().into_bigdecimal();

        let transaction = TradeTransaction::new(
            tx_id.clone(),
            self.batch_id.clone(),
            from_token.clone(),
            from_amount_bd.clone(),
            to_token.clone(),
            to_amount_bd.clone(),
            price_bd,
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

    #[allow(dead_code)]
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
                    trade.from_token,
                    trade.from_amount.into_bigdecimal(),
                    trade.to_token,
                    trade.to_amount.into_bigdecimal(),
                    trade.price_yocto_near.into_bigdecimal(),
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
    pub async fn get_portfolio_value(&self) -> Result<BigDecimal> {
        TradeTransaction::get_portfolio_value_by_batch_async(self.batch_id.clone())
            .await
            .context("Failed to get portfolio value")
    }

    #[allow(dead_code)]
    pub async fn get_batch_transactions(&self) -> Result<Vec<TradeTransaction>> {
        TradeTransaction::find_by_batch_id_async(self.batch_id.clone())
            .await
            .context("Failed to get batch transactions")
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TradeData {
    pub tx_id: String,
    pub from_token: String,
    pub from_amount: YoctoAmount,
    pub to_token: String,
    pub to_amount: YoctoAmount,
    pub price_yocto_near: YoctoValue,
}

impl TradeData {
    #[allow(dead_code)]
    pub fn new(
        tx_id: String,
        from_token: String,
        from_amount: YoctoAmount,
        to_token: String,
        to_amount: YoctoAmount,
        price_yocto_near: YoctoValue,
    ) -> Self {
        Self {
            tx_id,
            from_token,
            from_amount,
            to_token,
            to_amount,
            price_yocto_near,
        }
    }
}

#[allow(dead_code)]
pub async fn get_latest_portfolio_value() -> Result<Option<BigDecimal>> {
    let latest_batch = TradeTransaction::get_latest_batch_id_async().await?;

    match latest_batch {
        Some(batch_id) => {
            let value = TradeTransaction::get_portfolio_value_by_batch_async(batch_id).await?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[allow(dead_code)]
pub async fn get_portfolio_timeline() -> Result<Vec<(String, BigDecimal, chrono::NaiveDateTime)>> {
    let timeline = TradeTransaction::get_portfolio_timeline_async().await?;

    Ok(timeline
        .into_iter()
        .map(|(batch_id, value, timestamp)| {
            (
                batch_id,
                value.unwrap_or_else(|| BigDecimal::from(0)),
                timestamp,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_trade_recorder() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;
        use bigdecimal::BigDecimal;

        // 評価期間を作成（外部キー制約のため）
        let new_period =
            NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;

        let recorder = TradeRecorder::new(period_id);
        let batch_id = recorder.get_batch_id().to_string();

        let tx_id = format!("test_tx_{}", Uuid::new_v4());
        let result = recorder
            .record_trade(
                tx_id.clone(),
                "wrap.near".to_string(),
                YoctoAmount::new(1000000000000000000000000u128),
                "akaia.tkn.near".to_string(),
                YoctoAmount::new(50000000000000000000000u128),
                YoctoValue::from_yocto(BigDecimal::from(20000000000000000000i128)),
            )
            .await
            .unwrap();

        assert_eq!(result.tx_id, tx_id);
        assert_eq!(result.trade_batch_id, batch_id);

        let portfolio_value = recorder.get_portfolio_value().await.unwrap();
        assert_eq!(portfolio_value, BigDecimal::from(20000000000000000000i128));

        // Cleanup
        crate::persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_batch_recording() {
        use crate::persistence::evaluation_period::NewEvaluationPeriod;
        use bigdecimal::BigDecimal;

        // 評価期間を作成（外部キー制約のため）
        let new_period =
            NewEvaluationPeriod::new(BigDecimal::from(100000000000000000000000000i128), vec![]);
        let created_period = new_period.insert_async().await.unwrap();
        let period_id = created_period.period_id;

        let recorder = TradeRecorder::new(period_id);

        let trades = vec![
            TradeData::new(
                format!("test_tx1_{}", Uuid::new_v4()),
                "wrap.near".to_string(),
                YoctoAmount::new(1000000000000000000000000u128),
                "token1.near".to_string(),
                YoctoAmount::new(50000000000000000000000u128),
                YoctoValue::from_yocto(BigDecimal::from(20000000000000000000i128)),
            ),
            TradeData::new(
                format!("test_tx2_{}", Uuid::new_v4()),
                "wrap.near".to_string(),
                YoctoAmount::new(2000000000000000000000000u128),
                "token2.near".to_string(),
                YoctoAmount::new(100000000000000000000000u128),
                YoctoValue::from_yocto(BigDecimal::from(40000000000000000000i128)),
            ),
        ];

        let tx_ids: Vec<String> = trades.iter().map(|t| t.tx_id.clone()).collect();
        let results = recorder.record_batch(trades).await.unwrap();

        assert_eq!(results.len(), 2);

        let portfolio_value = recorder.get_portfolio_value().await.unwrap();
        assert_eq!(portfolio_value, BigDecimal::from(60000000000000000000i128));

        // Cleanup
        for tx_id in tx_ids {
            crate::persistence::trade_transaction::TradeTransaction::delete_by_tx_id_async(tx_id)
                .await
                .unwrap();
        }
    }
}
