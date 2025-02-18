use super::{AccessKeyInfo, BlockInfo, Client, SentTx, TxInfo, ViewContract};
use crate::jsonrpc::client::StandardClient;
use crate::logging::*;
use crate::types::gas_price::GasPrice;
use crate::Result;
use anyhow::{anyhow, bail};
use near_crypto::InMemorySigner;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::action::{Action, FunctionCallAction, TransferAction};
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::{AccountId, Balance, BlockId, Finality};
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum,
    FinalExecutionStatus, QueryRequest, TxExecutionStatus,
};
use near_sdk::Gas;

#[derive(Debug, Clone)]
pub struct StandardDelegate {
    pub(super) client: StandardClient,
}

impl StandardDelegate {
    async fn send_tx(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        actions: &[Action],
    ) -> Result<impl SentTx> {
        let log = DEFAULT.new(o!(
            "function" => "send_tx",
            "signer" => format!("{}", signer.account_id),
            "receiver" => format!("{}", receiver),
        ));

        let access_key = self.get_access_key_info(signer).await?;
        let block = self.get_recent_block().await?;
        let nonce = access_key.nonce + 1;
        let block_hash = block.header.hash;

        let transaction = Transaction::V0(TransactionV0 {
            signer_id: signer.account_id.clone(),
            public_key: signer.public_key(),
            nonce,
            receiver_id: receiver.clone(),
            block_hash,
            actions: actions.to_vec(),
        });

        let (hash, _) = transaction.get_hash_and_size();
        let signature = signer.sign(hash.as_bytes());
        let signed_tx = SignedTransaction::new(signature, transaction);

        let req = methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {
            signed_transaction: signed_tx,
        };

        let res = self.client.call(req).await?;
        info!(log, "broadcasted";
            "response" => %res,
            "nonce" => nonce,
            "block_hash" => %block_hash,
            "public_key" => %signer.public_key(),
        );
        Ok(StandardSentTx {
            tx_info: self.clone(),
            account: signer.account_id.clone(),
            tx_hash: res,
        })
    }
}

impl BlockInfo for StandardDelegate {
    async fn get_recent_block(&self) -> Result<BlockView> {
        let req = methods::block::RpcBlockRequest {
            block_reference: Finality::Final.into(),
        };
        let res = self.client.call(req).await?;
        Ok(res)
    }
}

impl super::GasInfo for StandardDelegate {
    async fn get_gas_price(&self, block: Option<BlockId>) -> Result<GasPrice> {
        let req = methods::gas_price::RpcGasPriceRequest { block_id: block };
        let res = self.client.call(req).await?;
        Ok(GasPrice::from_balance(res.gas_price))
    }
}

impl super::AccountInfo for StandardDelegate {
    async fn get_native_amount(&self, account: &AccountId) -> Result<Balance> {
        let req = methods::query::RpcQueryRequest {
            block_reference: Finality::Final.into(),
            request: QueryRequest::ViewAccount {
                account_id: account.clone(),
            },
        };
        let res = self.client.call(req).await?;
        if let QueryResponseKind::ViewAccount(am) = res.kind {
            Ok(am.amount)
        } else {
            panic!("View account is not view account")
        }
    }
}

impl AccessKeyInfo for StandardDelegate {
    async fn get_access_key_info(&self, signer: &InMemorySigner) -> Result<AccessKeyView> {
        let req = methods::query::RpcQueryRequest {
            block_reference: Finality::Final.into(),
            request: QueryRequest::ViewAccessKey {
                account_id: signer.account_id.clone(),
                public_key: signer.public_key(),
            },
        };
        let res = self.client.call(req).await?;
        match res.kind {
            QueryResponseKind::AccessKey(access_key) => Ok(access_key),
            _ => panic!("unexpected response"),
        }
    }
}

impl ViewContract for StandardDelegate {
    async fn view_contract<T>(
        &self,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
    ) -> Result<CallResult>
    where
        T: ?Sized + serde::Serialize,
    {
        let req = methods::query::RpcQueryRequest {
            block_reference: Finality::Final.into(),
            request: QueryRequest::CallFunction {
                account_id: receiver.clone(),
                method_name: method_name.to_string(),
                args: serde_json::to_vec(args)?.into(),
            },
        };
        let res = self.client.call(req).await?;
        match res.kind {
            QueryResponseKind::CallResult(r) => Ok(r),
            _ => panic!("unexpected response"),
        }
    }
}

impl TxInfo for StandardDelegate {
    async fn wait_tx_result(
        &self,
        sender: &AccountId,
        tx_hash: &CryptoHash,
        wait_until: TxExecutionStatus,
    ) -> Result<RpcTransactionResponse> {
        let log = DEFAULT.new(o!(
            "function" => "wait_tx_result",
            "sender" => format!("{}", sender),
            "tx_hash" => format!("{}", tx_hash),
            "wait_until" => format!("{:?}", wait_until),
        ));
        info!(log, "asking for transaction status");
        let req = methods::tx::RpcTransactionStatusRequest {
            transaction_info: methods::tx::TransactionInfo::TransactionId {
                tx_hash: tx_hash.to_owned(),
                sender_account_id: sender.clone(),
            },
            wait_until,
        };
        let res = self.client.call(req).await?;
        info!(log, "Transaction status";
            "status" => format!("{:?}", res.final_execution_status),
        );
        Ok(res)
    }
}

impl super::SendTx for StandardDelegate {
    async fn transfer_native_token(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        amount: Balance,
    ) -> Result<impl SentTx> {
        let log = DEFAULT.new(o!(
            "function" => "transfer_native_token",
            "signer" => format!("{}", signer.account_id),
            "receiver" => format!("{}", receiver),
            "amount" => amount,
        ));
        info!(log, "transferring native token");
        let action = Action::Transfer(TransferAction { deposit: amount });

        self.send_tx(signer, receiver, &[action]).await
    }

    async fn exec_contract<T>(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
        deposit: Balance,
    ) -> Result<impl SentTx>
    where
        T: ?Sized + serde::Serialize,
    {
        let log = DEFAULT.new(o!(
            "function" => "exec_contract",
            "signer" => format!("{}", signer.account_id),
            "receiver" => format!("{}", receiver),
            "method_name" => format!("{}", method_name),
            "deposit" => deposit,
        ));
        info!(log, "executing contract");

        let action = Action::FunctionCall(
            FunctionCallAction {
                method_name: method_name.to_string(),
                args: serde_json::to_vec(&args)?,
                gas: Gas::from_tgas(300).as_gas(),
                deposit,
            }
            .into(),
        );

        self.send_tx(signer, receiver, &[action]).await
    }
}

pub struct StandardSentTx {
    tx_info: StandardDelegate,
    account: AccountId,
    tx_hash: CryptoHash,
}

impl SentTx for StandardSentTx {
    async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum> {
        self.tx_info
            .wait_tx_result(&self.account, &self.tx_hash, TxExecutionStatus::Executed)
            .await?
            .final_execution_outcome
            .ok_or_else(|| anyhow!("No outcome of tx: {}", self.tx_hash))
    }

    async fn wait_for_success(&self) -> Result<ExecutionOutcomeView> {
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
