#![allow(dead_code)]

use crate::config;
use crate::jsonrpc::{AccountInfo, SendTx, SentTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use crate::types::{MicroNear, MilliNear};
use crate::wallet;
use crate::Result;
use anyhow::anyhow;
use futures_util::FutureExt;
use near_crypto::InMemorySigner;
use near_primitives::transaction::Action;
use near_primitives::types::Balance;
use near_primitives::views::CallResult;
use near_primitives::views::{ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::str::FromStr;
use std::sync::Once;

const DEFAULT_REQUIRED_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const MINIMUM_NATIVE_BALANCE: Balance = NearToken::from_near(1).as_yoctonear();
const INTERVAL_OF_HARVEST: u64 = 24 * 60 * 60;

static LAST_HARVEST: AtomicU64 = AtomicU64::new(0);
static HARVEST_ACCOUNT: Lazy<AccountId> = Lazy::new(|| {
    let value = config::get("HARVEST_ACCOUNT_ID").unwrap_or_else(|err| panic!("{}", err));
    value
        .parse()
        .unwrap_or_else(|err| panic!("Failed to parse config `{}`: {}", value, err))
});

static INIT: Once = Once::new();

fn is_time_to_harvest() -> bool {
    let last = LAST_HARVEST.load(Ordering::Relaxed);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    now - last > INTERVAL_OF_HARVEST
}

fn update_last_harvest() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    LAST_HARVEST.store(now, Ordering::Relaxed);
}

pub async fn start<C>(client: &C) -> Result<(TokenAccount, Balance)>
where
    C: AccountInfo + SendTx + ViewContract,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.start",
    ));
    let required_balance = {
        let max = get_history().read().unwrap().inputs.max();
        if max.is_zero() {
            DEFAULT_REQUIRED_BALANCE
        } else {
            max
        }
    };
    info!(log, "Starting balances";
        "required_balance" => %required_balance,
    );

    let token = WNEAR_TOKEN.clone();

    let wrapped_balance = balance_of_start_token(client, &token).await?;
    info!(log, "comparing";
        "wrapped_balance" => wrapped_balance,
    );

    if wrapped_balance < required_balance {
        refill(client, required_balance - wrapped_balance).await?;
        Ok((token, wrapped_balance))
    } else {
        let upper = required_balance << 4;
        if upper < wrapped_balance {
            let _ignore_response = harvest(client, &WNEAR_TOKEN, wrapped_balance - upper, upper)
                .map(|r| match r {
                    Ok(_) => info!(log, "successfully harvested"),
                    Err(err) => warn!(log, "failed to harvest: {}", err),
                });
        }
        Ok((token, upper))
    }
}

async fn balance_of_start_token<C: ViewContract>(
    client: &C,
    token: &TokenAccount,
) -> Result<Balance> {
    let account = wallet::WALLET.account_id();
    let deposits = deposit::get_deposits(client, account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill<C>(client: &C, want: Balance) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => format!("{}", want),
    ));
    let account = wallet::WALLET.account_id();
    let wrapped_balance = deposit::wnear::balance_of(client, account).await?;
    let log = log.new(o!(
        "wrapped_balance" => format!("{}", wrapped_balance),
    ));
    debug!(log, "checking");
    if wrapped_balance < want {
        let wrapping = want - wrapped_balance;
        let native_balance = client.get_native_amount(account).await?;
        let log = log.new(o!(
            "native_balance" => format!("{}", native_balance),
            "wrapping" => format!("{}", wrapping),
        ));
        debug!(log, "checking");
        let available = native_balance
            .checked_sub(MINIMUM_NATIVE_BALANCE)
            .unwrap_or_default();
        if available < wrapping {
            return Err(anyhow!(
                "Insufficient balance: required: {:?}, native_balance {}, {:?}, {:?}",
                MilliNear::from_yocto(want),
                native_balance,
                MilliNear::from_yocto(native_balance),
                MicroNear::from_yocto(native_balance),
            ));
        }
        info!(log, "wrapping");
        deposit::wnear::wrap(client, wrapping)
            .await?
            .wait_for_success()
            .await?;
    }
    info!(log, "refilling";
        "amount" => %want,
    );
    deposit::deposit(client, &WNEAR_TOKEN, want)
        .await?
        .wait_for_success()
        .await?;
    Ok(())
}

// トレイトとして定義
pub trait Wallet {
    fn account_id(&self) -> &AccountId;
    fn signer(&self) -> &InMemorySigner;
}

// 実装を変更して、トレイトを使用するように
async fn harvest<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
    withdraw: Balance,
    required: Balance,
) -> Result<()>
where
    C: AccountInfo + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.harvest",
        "withdraw" => format!("{}", withdraw),
        "required" => format!("{}", required),
    ));
    info!(log, "withdrawing";
        "token" => %token,
    );
    deposit::withdraw(client, token, withdraw)
        .await?
        .wait_for_success()
        .await?;
    let account = wallet.account_id();
    let native_balance = client.get_native_amount(account).await?;
    let upper = required << 4;
    info!(log, "checking";
        "native_balance" => %native_balance,
        "upper" => %upper,
    );
    if upper < native_balance && is_time_to_harvest() {
        let amount = native_balance - upper;
        let target = &*HARVEST_ACCOUNT;
        info!(log, "harvesting";
            "target" => %target,
            "amount" => %amount,
        );
        let signer = wallet.signer();
        client
            .transfer_native_token(signer, target, amount)
            .await?
            .wait_for_success()
            .await?;
        update_last_harvest()
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    // テスト用のモックウォレット
    #[derive(Clone)]
    struct MockWallet {
        account_id: AccountId,
        signer: InMemorySigner,
    }

    impl MockWallet {
        fn new() -> Self {
            let account_id = AccountId::from_str("test.near").unwrap();
            let signer = InMemorySigner::from_seed(
                account_id.clone(),
                near_crypto::KeyType::ED25519,
                "test.near",
            );
            Self { account_id, signer }
        }

        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn signer(&self) -> &InMemorySigner {
            &self.signer
        }
    }

    // テスト用の初期化
    fn initialize() {
        INIT.call_once(|| {
            // 環境変数の設定
            std::env::set_var("ROOT_ACCOUNT_ID", "test.near");
            std::env::set_var("ROOT_MNEMONIC", "test test test test test test test test test test test junk");
            std::env::set_var("ROOT_HDPATH", "m/44'/397'/0'");
            std::env::set_var("HARVEST_ACCOUNT_ID", "harvest.near");
        });
    }

    // シンプルなモッククライアント
    struct MockClient {
        native_amount: Balance,
    }

    impl MockClient {
        fn new(native_amount: Balance) -> Self {
            Self { native_amount }
        }
    }

    // シンプルなモックトランザクション
    #[derive(Clone)]
    struct MockSentTx;

    impl SentTx for MockSentTx {
        async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum> {
            Ok(FinalExecutionOutcomeViewEnum::FinalExecutionOutcome(
                near_primitives::views::FinalExecutionOutcomeView {
                    status: near_primitives::views::FinalExecutionStatus::SuccessValue(vec![]),
                    transaction: near_primitives::views::SignedTransactionView {
                        signer_id: AccountId::try_from("test.near".to_string()).unwrap(),
                        public_key: near_crypto::PublicKey::empty(near_crypto::KeyType::ED25519),
                        nonce: 0,
                        receiver_id: AccountId::try_from("test.near".to_string()).unwrap(),
                        actions: vec![],
                        signature: near_crypto::Signature::empty(near_crypto::KeyType::ED25519),
                        hash: Default::default(),
                        priority_fee: 0,
                    },
                    transaction_outcome: near_primitives::views::ExecutionOutcomeWithIdView {
                        proof: vec![],
                        block_hash: Default::default(),
                        id: Default::default(),
                        outcome: ExecutionOutcomeView {
                            logs: vec![],
                            receipt_ids: vec![],
                            gas_burnt: 0,
                            tokens_burnt: 0,
                            executor_id: AccountId::try_from("test.near".to_string()).unwrap(),
                            status: near_primitives::views::ExecutionStatusView::SuccessValue(vec![]),
                            metadata: near_primitives::views::ExecutionMetadataView {
                                version: 1,
                                gas_profile: None,
                            },
                        },
                    },
                    receipts_outcome: vec![],
                }
            ))
        }

        async fn wait_for_success(&self) -> Result<ExecutionOutcomeView> {
            Ok(ExecutionOutcomeView {
                logs: vec![],
                receipt_ids: vec![],
                gas_burnt: 0,
                tokens_burnt: 0,
                executor_id: AccountId::try_from("test.near".to_string()).unwrap(),
                status: near_primitives::views::ExecutionStatusView::SuccessValue(vec![]),
                metadata: near_primitives::views::ExecutionMetadataView {
                    version: 1,
                    gas_profile: None,
                },
            })
        }
    }

    impl AccountInfo for MockClient {
        async fn get_native_amount(&self, _account: &AccountId) -> Result<Balance> {
            Ok(self.native_amount)
        }
    }

    impl SendTx for MockClient {
        type Output = MockSentTx;

        async fn transfer_native_token(
            &self,
            _signer: &InMemorySigner,
            _receiver: &AccountId,
            _amount: Balance,
        ) -> Result<Self::Output> {
            Ok(MockSentTx)
        }

        async fn exec_contract<T>(
            &self,
            _signer: &InMemorySigner,
            _receiver: &AccountId,
            _method_name: &str,
            _args: T,
            _deposit: Balance,
        ) -> Result<Self::Output>
        where
            T: Sized + serde::Serialize,
        {
            Ok(MockSentTx)
        }

        async fn send_tx(
            &self,
            _signer: &InMemorySigner,
            _receiver: &AccountId,
            _actions: Vec<Action>,
        ) -> Result<Self::Output> {
            Ok(MockSentTx)
        }
    }

    impl ViewContract for MockClient {
        async fn view_contract<T>(
            &self,
            _receiver: &AccountId,
            _method_name: &str,
            _args: &T,
        ) -> Result<CallResult>
        where
            T: ?Sized + serde::Serialize,
        {
            Ok(CallResult {
                result: vec![],
                logs: vec![],
            })
        }
    }

    impl Wallet for MockWallet {
        fn account_id(&self) -> &AccountId {
            &self.account_id
        }

        fn signer(&self) -> &InMemorySigner {
            &self.signer
        }
    }

    #[tokio::test]
    async fn test_harvest_with_sufficient_balance() {
        initialize();

        let required = 1_000_000;
        let native_balance = required << 5;
        let client = MockClient::new(native_balance);
        let wallet = MockWallet::new();

        LAST_HARVEST.store(0, Ordering::Relaxed);

        let result = harvest(&client, &wallet, &WNEAR_TOKEN, 1000, required).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_harvest_with_insufficient_balance() {
        initialize();

        let required = 1_000_000;
        let native_balance = required << 3;
        let client = MockClient::new(native_balance);
        let wallet = MockWallet::new();

        let result = harvest(&client, &wallet, &WNEAR_TOKEN, 1000, required).await;
        assert!(result.is_ok());
    }
}
