#![allow(dead_code)]

use crate::config;
use crate::jsonrpc::{AccountInfo, SendTx, SentTx, ViewContract};
use crate::logging::*;
use crate::ref_finance::deposit;
use crate::ref_finance::history::get_history;
use crate::ref_finance::token_account::{TokenAccount, WNEAR_TOKEN};
use crate::types::{MicroNear, MilliNear};
use crate::wallet::Wallet;
use crate::Result;
use anyhow::anyhow;
use futures_util::FutureExt;
use near_primitives::types::Balance;
use near_sdk::{AccountId, NearToken};
use num_traits::Zero;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
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

pub async fn start<C, W>(client: &C, wallet: &W) -> Result<(TokenAccount, Balance)>
where
    C: AccountInfo + SendTx + ViewContract,
    W: Wallet,
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

    let wrapped_balance = balance_of_start_token(client, wallet, &token).await?;
    info!(log, "comparing";
        "wrapped_balance" => wrapped_balance,
    );

    if wrapped_balance < required_balance {
        refill(client, wallet, required_balance - wrapped_balance).await?;
        Ok((token, wrapped_balance))
    } else {
        let upper = required_balance << 4;
        if upper < wrapped_balance {
            let _ignore_response =
                harvest(client, wallet, &WNEAR_TOKEN, wrapped_balance - upper, upper).map(|r| {
                    match r {
                        Ok(_) => info!(log, "successfully harvested"),
                        Err(err) => warn!(log, "failed to harvest: {}", err),
                    }
                });
        }
        Ok((token, upper))
    }
}

async fn balance_of_start_token<C, W>(
    client: &C,
    wallet: &W,
    token: &TokenAccount,
) -> Result<Balance>
where
    C: AccountInfo + ViewContract,
    W: Wallet,
{
    let account = wallet.account_id();
    let deposits = deposit::get_deposits(client, account).await?;
    Ok(deposits.get(token).map(|u| u.0).unwrap_or_default())
}

async fn refill<C, W>(client: &C, wallet: &W, want: Balance) -> Result<()>
where
    C: AccountInfo + ViewContract + SendTx,
    W: Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "balances.refill",
        "want" => format!("{}", want),
    ));
    let account = wallet.account_id();
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
        deposit::wnear::wrap(client, wallet, wrapping)
            .await?
            .wait_for_success()
            .await?;

        info!(log, "refilling";
            "amount" => %wrapping,
        );
        deposit::deposit(client, wallet, &WNEAR_TOKEN, wrapping)
            .await?
            .wait_for_success()
            .await?;
    }
    Ok(())
}

pub async fn harvest<C, W>(
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
    deposit::withdraw(client, wallet, token, withdraw)
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
    use near_crypto::InMemorySigner;
    use near_primitives::transaction::Action;
    use near_primitives::views::{CallResult, ExecutionOutcomeView, FinalExecutionOutcomeViewEnum};
    use near_sdk::json_types::U128;
    use serde_json::json;
    use std::cell::Cell;
    use std::sync::Once;

    static INIT: Once = Once::new();

    struct MockWallet {
        account_id: AccountId,
        signer: InMemorySigner,
    }

    impl MockWallet {
        fn new() -> Self {
            let account_id: AccountId = "test.near".parse().unwrap();
            let signer = InMemorySigner::from_seed(
                account_id.clone(),
                near_crypto::KeyType::ED25519,
                "test.near",
            );
            Self { account_id, signer }
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

    fn initialize() {
        INIT.call_once(|| {
            std::env::set_var("HARVEST_ACCOUNT_ID", "harvest.near");
        });
    }

    struct MockClient {
        native_amount: Cell<Balance>,
        wnear_amount: Cell<Balance>,
        wnear_deposited: Cell<Balance>,
    }

    impl AccountInfo for MockClient {
        async fn get_native_amount(&self, _account: &AccountId) -> Result<Balance> {
            Ok(self.native_amount.get())
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
            method_name: &str,
            args: T,
            deposit: Balance,
        ) -> Result<Self::Output>
        where
            T: Sized + serde::Serialize,
        {
            match method_name {
                "near_deposit" => {
                    self.native_amount
                        .set(self.native_amount.get().saturating_sub(deposit));
                    self.wnear_amount
                        .set(self.wnear_amount.get().saturating_add(deposit));
                }
                "ft_transfer_call" => {
                    let args_str = serde_json::to_string(&args).unwrap();
                    let args_value: serde_json::Value = serde_json::from_str(&args_str).unwrap();
                    let amount: Balance = args_value["amount"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default();
                    self.wnear_amount
                        .set(self.wnear_amount.get().saturating_sub(amount));
                    self.wnear_deposited
                        .set(self.wnear_deposited.get().saturating_add(amount));
                }
                _ => {}
            }
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
            method_name: &str,
            _args: &T,
        ) -> Result<CallResult>
        where
            T: ?Sized + serde::Serialize,
        {
            let result = match method_name {
                "get_deposits" => {
                    let deposits = json!({
                        WNEAR_TOKEN.to_string(): U128(self.wnear_deposited.get()),
                    });
                    serde_json::to_vec(&deposits)?
                }
                "ft_balance_of" => {
                    let balance = U128(self.wnear_amount.get());
                    serde_json::to_vec(&balance)?
                }
                _ => {
                    let balance = U128(0);
                    serde_json::to_vec(&balance)?
                }
            };

            Ok(CallResult {
                result,
                logs: vec![],
            })
        }
    }

    struct MockSentTx;

    impl SentTx for MockSentTx {
        async fn wait_for_executed(&self) -> Result<FinalExecutionOutcomeViewEnum> {
            unimplemented!()
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

    #[tokio::test]
    async fn test_start() {
        initialize();

        let client = MockClient {
            native_amount: Cell::new(DEFAULT_REQUIRED_BALANCE << 5),
            wnear_amount: Cell::new(0),
            wnear_deposited: Cell::new(0),
        };
        let wallet = MockWallet::new();

        let result = start(&client, &wallet).await;
        let (token, balance) = result.unwrap();
        assert_eq!(token, WNEAR_TOKEN.clone());
        assert!(balance.is_zero());
    }

    #[tokio::test]
    async fn test_harvest_with_sufficient_balance() {
        initialize();

        let required = 1_000_000;
        let native_balance = required << 5;
        let client = MockClient {
            native_amount: Cell::new(native_balance),
            wnear_amount: Cell::new(0),
            wnear_deposited: Cell::new(0),
        };
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
        let client = MockClient {
            native_amount: Cell::new(native_balance),
            wnear_amount: Cell::new(0),
            wnear_deposited: Cell::new(0),
        };
        let wallet = MockWallet::new();

        let result = harvest(&client, &wallet, &WNEAR_TOKEN, 1000, required).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_time_to_harvest() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST - 1, Ordering::Relaxed);
        assert!(is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 1, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
        LAST_HARVEST.store(now - INTERVAL_OF_HARVEST + 2, Ordering::Relaxed);
        assert!(!is_time_to_harvest());
    }

    #[test]
    fn test_update_last_harvest() {
        LAST_HARVEST.store(0, Ordering::Relaxed);
        update_last_harvest();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(now, LAST_HARVEST.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_refill_with_sufficient_wrapped_balance() {
        initialize();

        let required = 1_000_000;
        let client = MockClient {
            native_amount: Cell::new(required << 1),
            wnear_amount: Cell::new(required << 1),
            wnear_deposited: Cell::new(required << 1),
        };
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_refill_with_sufficient_native_balance() {
        initialize();

        let required = NearToken::from_near(2).as_yoctonear();
        let client = MockClient {
            native_amount: Cell::new(required << 2),
            wnear_amount: Cell::new(0),
            wnear_deposited: Cell::new(0),
        };
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_refill_with_insufficient_native_balance() {
        initialize();

        let required = 1_000_000;
        let client = MockClient {
            native_amount: Cell::new(MINIMUM_NATIVE_BALANCE),
            wnear_amount: Cell::new(0),
            wnear_deposited: Cell::new(0),
        };
        let wallet = MockWallet::new();

        let result = refill(&client, &wallet, required).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_u128_serialization() {
        let amount = U128(1_000_000);
        let args = json!({
            "receiver_id": "contract.near",
            "amount": amount,
            "msg": "",
        });
        println!("args: {:?}", args);
        let args_str = serde_json::to_string(&args).unwrap();
        println!("args_str: {}", args_str);
        let args_value: serde_json::Value = serde_json::from_str(&args_str).unwrap();
        println!("args_value: {:?}", args_value);
    }
}
