use crate::jsonrpc::{SentTx, TxInfo};
use crate::logging::*;
use anyhow::{anyhow, bail};
use near_primitives::hash::CryptoHash;
use near_primitives::types::AccountId;
use near_primitives::views::{
    ExecutionOutcomeView, FinalExecutionOutcomeViewEnum, FinalExecutionStatus, TxExecutionStatus,
};
use std::time::Duration;

pub struct StandardSentTx<A> {
    tx_info: A,
    account: AccountId,
    tx_hash: CryptoHash,
}

impl<A> std::fmt::Display for StandardSentTx<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Tx(account={}, hash={})", self.account, self.tx_hash)
    }
}

impl<A> StandardSentTx<A> {
    pub fn new(tx_info: A, account: AccountId, tx_hash: CryptoHash) -> Self {
        Self {
            tx_info,
            account,
            tx_hash,
        }
    }
}

impl<A: TxInfo> SentTx for StandardSentTx<A> {
    async fn wait_for_executed(&self) -> crate::Result<FinalExecutionOutcomeViewEnum> {
        self.tx_info
            .wait_tx_result(&self.account, &self.tx_hash, TxExecutionStatus::Executed)
            .await?
            .final_execution_outcome
            .ok_or_else(|| anyhow!("No outcome of tx: {}", self.tx_hash))
    }

    async fn wait_for_success(&self) -> crate::Result<ExecutionOutcomeView> {
        let log = DEFAULT.new(o!(
            "function" => "wait_for_success",
            "tx_hash" => format!("{}", self.tx_hash),
            "account" => format!("{}", self.account),
        ));

        info!(
            log,
            "starting client-side polling for transaction completion"
        );

        // クライアント側ポーリング設定
        const MAX_ATTEMPTS: u32 = 30; // 最大30回試行
        const POLL_INTERVAL: Duration = Duration::from_secs(2); // 2秒間隔

        for attempt in 1..=MAX_ATTEMPTS {
            let attempt_log = log.new(o!("attempt" => attempt));
            debug!(attempt_log, "polling transaction status");

            // wait_until=NONE でステータスを取得
            let response = self
                .tx_info
                .wait_tx_result(&self.account, &self.tx_hash, TxExecutionStatus::None)
                .await?;

            if let Some(outcome) = response.final_execution_outcome {
                debug!(attempt_log, "transaction outcome received");

                let view = match outcome {
                    FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(view) => view,
                    FinalExecutionOutcomeViewEnum::FinalExecutionOutcomeWithReceipt(view) => {
                        view.final_outcome
                    }
                };

                match view.status {
                    FinalExecutionStatus::NotStarted | FinalExecutionStatus::Started => {
                        debug!(attempt_log, "transaction still pending"; "status" => format!("{:?}", view.status));
                        // まだ実行中 - 次のポーリングへ
                    }
                    FinalExecutionStatus::Failure(err) => {
                        info!(attempt_log, "transaction failed"; "error" => format!("{:?}", err));
                        return Err(err.into());
                    }
                    FinalExecutionStatus::SuccessValue(_) => {
                        info!(attempt_log, "transaction completed successfully");
                        return Ok(view.transaction_outcome.outcome);
                    }
                }
            } else {
                debug!(attempt_log, "no outcome yet, transaction may be in mempool");
            }

            if attempt < MAX_ATTEMPTS {
                debug!(attempt_log, "waiting before next poll"; "interval_secs" => POLL_INTERVAL.as_secs());
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }

        let err_msg = format!(
            "Transaction polling timeout after {} attempts ({} seconds)",
            MAX_ATTEMPTS,
            MAX_ATTEMPTS as u64 * POLL_INTERVAL.as_secs()
        );
        info!(log, "{}", err_msg);
        bail!(err_msg)
    }
}
