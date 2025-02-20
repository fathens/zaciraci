use crate::jsonrpc::{SentTx, TxInfo};
use anyhow::{anyhow, bail};
use near_primitives::hash::CryptoHash;
use near_primitives::types::AccountId;
use near_primitives::views::{
    ExecutionOutcomeView, FinalExecutionOutcomeViewEnum, FinalExecutionStatus, TxExecutionStatus,
};

pub struct StandardSentTx<A> {
    tx_info: A,
    account: AccountId,
    tx_hash: CryptoHash,
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
        let view = match self.wait_for_executed().await? {
            FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(view) => view,
            FinalExecutionOutcomeViewEnum::FinalExecutionOutcomeWithReceipt(view) => {
                view.final_outcome
            }
        };
        match view.status {
            FinalExecutionStatus::NotStarted => bail!("tx must be executed"),
            FinalExecutionStatus::Started => bail!("tx must be executed"),
            FinalExecutionStatus::Failure(err) => Err(err.into()),
            FinalExecutionStatus::SuccessValue(_) => Ok(view.transaction_outcome.outcome),
        }
    }
}
