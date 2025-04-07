use super::{AccessKeyInfo, BlockInfo, RpcClient, SendTx, TxInfo, ViewContract};
use crate::jsonrpc::sent_tx::StandardSentTx;
use crate::logging::*;
use crate::types::gas_price::GasPrice;
use crate::Result;
use near_crypto::InMemorySigner;
use near_jsonrpc_client::methods;
use near_jsonrpc_primitives::types::query::QueryResponseKind;
use near_jsonrpc_primitives::types::transactions::RpcTransactionResponse;
use near_primitives::action::{Action, FunctionCallAction, TransferAction};
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::{AccountId, Balance, BlockId, Finality};
use near_primitives::views::{
    AccessKeyView, BlockView, CallResult, QueryRequest, TxExecutionStatus,
};
use near_sdk::Gas;
use std::sync::Arc;

#[derive(Debug)]
pub struct StandardNearClient<A> {
    rpc: Arc<A>,
}

impl<A> StandardNearClient<A> {
    pub fn new(rpc: &Arc<A>) -> Self {
        Self {
            rpc: Arc::clone(rpc),
        }
    }
}

impl<A> Clone for StandardNearClient<A> {
    fn clone(&self) -> Self {
        Self {
            rpc: Arc::clone(&self.rpc),
        }
    }
}

impl<A: RpcClient> BlockInfo for StandardNearClient<A> {
    async fn get_recent_block(&self) -> Result<BlockView> {
        let req = methods::block::RpcBlockRequest {
            block_reference: Finality::Final.into(),
        };
        let res = self.rpc.call(req).await?;
        Ok(res)
    }
}

impl<A: RpcClient> super::GasInfo for StandardNearClient<A> {
    async fn get_gas_price(&self, block: Option<BlockId>) -> Result<GasPrice> {
        let req = methods::gas_price::RpcGasPriceRequest { block_id: block };
        let res = self.rpc.call(req).await?;
        Ok(GasPrice::from_balance(res.gas_price))
    }
}

impl<A: RpcClient> super::AccountInfo for StandardNearClient<A> {
    async fn get_native_amount(&self, account: &AccountId) -> Result<Balance> {
        let req = methods::query::RpcQueryRequest {
            block_reference: Finality::Final.into(),
            request: QueryRequest::ViewAccount {
                account_id: account.clone(),
            },
        };
        let res = self.rpc.call(req).await?;
        if let QueryResponseKind::ViewAccount(am) = res.kind {
            Ok(am.amount)
        } else {
            panic!("View account is not view account")
        }
    }
}

impl<A: RpcClient> AccessKeyInfo for StandardNearClient<A> {
    async fn get_access_key_info(&self, signer: &InMemorySigner) -> Result<AccessKeyView> {
        let req = methods::query::RpcQueryRequest {
            block_reference: Finality::Final.into(),
            request: QueryRequest::ViewAccessKey {
                account_id: signer.account_id.clone(),
                public_key: signer.public_key(),
            },
        };
        let res = self.rpc.call(req).await?;
        match res.kind {
            QueryResponseKind::AccessKey(access_key) => Ok(access_key),
            _ => panic!("unexpected response"),
        }
    }
}

impl<A> ViewContract for StandardNearClient<A> 
where
    A: RpcClient + std::marker::Sync + std::marker::Send,
{
    fn view_contract<T>(
        &self,
        receiver: &AccountId,
        method_name: &str,
        args: &T,
    ) -> impl std::future::Future<Output = Result<CallResult>> + Send
    where
        T: ?Sized + serde::Serialize + std::marker::Sync,
    {
        async move {
            let req = methods::query::RpcQueryRequest {
                block_reference: Finality::Final.into(),
                request: QueryRequest::CallFunction {
                    account_id: receiver.clone(),
                    method_name: method_name.to_string(),
                    args: serde_json::to_vec(args)?.into(),
                },
            };
            let res = self.rpc.call(req).await?;
            match res.kind {
                QueryResponseKind::CallResult(r) => Ok(r),
                _ => panic!("unexpected response"),
            }
        }
    }
}

impl<A: RpcClient> TxInfo for StandardNearClient<A> {
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
        let res = self.rpc.call(req).await?;
        info!(log, "Transaction status";
            "status" => format!("{:?}", res.final_execution_status),
        );
        Ok(res)
    }
}

impl<A: RpcClient> SendTx for StandardNearClient<A> {
    type Output = StandardSentTx<Self>;

    async fn transfer_native_token(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        amount: Balance,
    ) -> Result<Self::Output> {
        let log = DEFAULT.new(o!(
            "function" => "transfer_native_token",
            "signer" => format!("{}", signer.account_id),
            "receiver" => format!("{}", receiver),
            "amount" => amount,
        ));
        info!(log, "transferring native token");
        let action = Action::Transfer(TransferAction { deposit: amount });

        self.send_tx(signer, receiver, vec![action]).await
    }

    async fn exec_contract<T>(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        method_name: &str,
        args: T,
        deposit: Balance,
    ) -> Result<Self::Output>
    where
        T: Sized + serde::Serialize,
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

        self.send_tx(signer, receiver, vec![action]).await
    }

    async fn send_tx(
        &self,
        signer: &InMemorySigner,
        receiver: &AccountId,
        actions: Vec<Action>,
    ) -> Result<Self::Output> {
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
            actions,
        });

        let (hash, _) = transaction.get_hash_and_size();
        let signature = signer.sign(hash.as_bytes());
        let signed_tx = SignedTransaction::new(signature, transaction);

        let req = methods::broadcast_tx_async::RpcBroadcastTxAsyncRequest {
            signed_transaction: signed_tx,
        };

        let res = self.rpc.call(req).await?;
        info!(log, "broadcasted";
            "response" => %res,
            "nonce" => nonce,
            "block_hash" => %block_hash,
            "public_key" => %signer.public_key(),
        );
        let sent_tx = StandardSentTx::new(self.clone(), signer.account_id.clone(), res);
        Ok(sent_tx)
    }
}
