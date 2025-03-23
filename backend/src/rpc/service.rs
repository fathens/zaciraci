use super::ZaciraciService;
use crate::jsonrpc::{AccountInfo, GasInfo, SendTx, SentTx};
use crate::ref_finance::pool_info::TokenPairLike;
use crate::ref_finance::token_account::TokenAccount;
use crate::types::{MicroNear, MilliNear};
use crate::wallet::Wallet;
use crate::{jsonrpc, ref_finance, wallet};
use num_rational::Ratio;
use num_traits::ToPrimitive;
use tarpc::context;

// サービスの実装
#[derive(Clone)]
pub struct ZaciraciServiceImpl;

impl ZaciraciService for ZaciraciServiceImpl {
    // サーバーの健全性チェック
    async fn healthcheck(self, _: context::Context) -> String {
        "OK".to_string()
    }
    
    // ネイティブトークンの残高を取得
    async fn native_token_balance(self, _: context::Context) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let account = wallet.account_id();
        let res = client.get_native_amount(account).await;
        match res {
            Ok(balance) => {
                format!("Balance: {balance:?}\n")
            }
            Err(err) => {
                format!("Error: {err}")
            }
        }
    }
    
    // ネイティブトークンを転送
    async fn native_token_transfer(self, _: context::Context, receiver: String, amount: String) -> String {
        let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
        let amount = MicroNear::of(amount_micro).to_yocto();
        let receiver = receiver.parse().unwrap();
        let wallet = wallet::new_wallet();
        let signer = wallet.signer();
        let client = jsonrpc::new_client();
        let res = client
            .transfer_native_token(signer, &receiver, amount)
            .await;
        match res {
            Ok(_) => "OK".to_owned(),
            Err(err) => {
                format!("Error: {err}")
            }
        }
    }
    
    // すべてのプールを取得
    async fn get_all_pools(self, _: context::Context) -> String {
        let client = jsonrpc::new_client();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        format!("Pools: {}", pools.len())
    }
    
    // リターンを推定
    async fn estimate_return(self, _: context::Context, pool_id: u32, amount: u128) -> String {
        use crate::ref_finance::errors::Error;

        let client = jsonrpc::new_client();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let pool = pools.get(pool_id).unwrap();
        let n = pool.len();
        assert!(n > 1, "{}", Error::InvalidPoolSize(n));
        let token_in = 0;
        let token_out = n - 1;
        let amount_in = amount;
        let pair = pool.get_pair(token_in.into(), token_out.into()).unwrap();
        let amount_out = pair.estimate_return(amount_in).unwrap();
        let token_a = pair.token_in_id();
        let token_b = pair.token_out_id();
        format!("Estimated: {token_a}({amount_in}) -> {token_b}({amount_out})")
    }
    
    // リターンを取得
    async fn get_return(self, _: context::Context, pool_id: u32, amount: u128) -> String {
        use crate::ref_finance::errors::Error;

        let client = jsonrpc::new_client();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let pool = pools.get(pool_id).unwrap();
        let n = pool.len();
        assert!(n > 1, "{}", Error::InvalidPoolSize(n));
        let token_in = 0;
        let token_out = n - 1;
        let amount_in = amount;
        let pair = pool.get_pair(token_in.into(), token_out.into()).unwrap();
        let token_a = pair.token_in_id();
        let token_b = pair.token_out_id();
        let amount_out = pair.get_return(&client, amount_in).await.unwrap();
        format!("Return: {token_a}({amount_in}) -> {token_b}({amount_out})")
    }
    
    // すべてのトークンをリスト
    async fn list_all_tokens(self, _: context::Context) -> String {
        let client = jsonrpc::new_client();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let tokens = ref_finance::path::all_tokens(pools);
        let mut tokens: Vec<_> = tokens.iter().map(|t| t.to_string()).collect();
        tokens.sort();
        let mut result = String::from("Tokens:\n");
        for token in tokens {
            result.push_str(&format!("{token}\n"));
        }
        result
    }
    
    // リターンをリスト
    async fn list_returns(self, _: context::Context, token_account: String, initial_value: String) -> String {
        let client = jsonrpc::new_client();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let graph = ref_finance::path::graph::TokenGraph::new(pools);
        let amount_in = MilliNear::of(initial_value.replace("_", "").parse().unwrap());
        let start: TokenAccount = token_account.parse().unwrap();
        let mut sorted_returns = ref_finance::path::sorted_returns(&graph, &start.into(), amount_in)
            .await
            .unwrap();
        sorted_returns.reverse();

        let mut result = String::from("from: {token_account}\n");
        for (goal, value, depth) in sorted_returns {
            let rational = Ratio::new(value.to_yocto(), amount_in.to_yocto());
            let ret = rational.to_f32().unwrap();
            result.push_str(&format!("{goal}: {ret}({depth})\n"));
        }
        result
    }
    
    // ゴールを選択
    async fn pick_goals(self, _: context::Context, token_account: String, initial_value: String) -> String {
        let client = jsonrpc::new_client();
        let gas_price = client.get_gas_price(None).await.unwrap();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let graph = ref_finance::path::graph::TokenGraph::new(pools);
        let amount_in: u32 = initial_value.replace("_", "").parse().unwrap();
        let start: TokenAccount = token_account.parse().unwrap();
        let goals =
            ref_finance::path::pick_goals(&graph, &start.into(), MilliNear::of(amount_in), gas_price)
                .await
                .unwrap();
        let mut result = String::from(&format!("from: {token_account}({amount_in})\n"));
        match goals {
            None => {
                result.push_str("No goals found\n");
            }
            Some(previews) => {
                for preview in previews {
                    let input_value = MicroNear::from_yocto(preview.input_value);
                    let token_name = preview.token.to_string();
                    let gain = MicroNear::from_yocto(preview.output_value - input_value.to_yocto());
                    result.push_str(&format!("{input_value:?} -> {token_name} -> {gain:?}\n"));
                }
            }
        }
        result
    }
    
    // スワップを実行
    async fn run_swap(self, _: context::Context, token_in_account: String, initial_value: String, token_out_account: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let pools = ref_finance::pool_info::PoolInfoList::read_from_node(&client)
            .await
            .unwrap();
        let graph = ref_finance::path::graph::TokenGraph::new(pools);
        let amount_in: u128 = initial_value.replace("_", "").parse().unwrap();
        let start_token: TokenAccount = token_in_account.parse().unwrap();
        let goal_token: TokenAccount = token_out_account.parse().unwrap();
        let start = &start_token.into();
        let goal = &goal_token.into();

        let path = ref_finance::path::swap_path(&graph, start, goal)
            .await
            .unwrap();
        let tokens = ref_finance::swap::gather_token_accounts(&[&path]);
        let res = ref_finance::storage::check_and_deposit(&client, &wallet, &tokens)
            .await
            .unwrap();
        if res.is_some() {
            return "no account to deposit".to_string();
        }

        let arg = ref_finance::swap::SwapArg {
            initial_in: amount_in,
            min_out: amount_in + MilliNear::of(1).to_yocto(),
        };
        let res = ref_finance::swap::run_swap(&client, &wallet, &path, arg).await;

        match res {
            Ok((tx_hash, value)) => {
                let outcome = tx_hash.wait_for_success().await.unwrap();
                format!("Result: {value:?} ({outcome:?})")
            }
            Err(e) => format!("Error: {e}"),
        }
    }
    
    // 最小ストレージ預金を取得
    async fn storage_deposit_min(self, _: context::Context) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let bounds = ref_finance::storage::check_bounds(&client).await.unwrap();
        let value = bounds.min.0;
        let res = ref_finance::storage::deposit(&client, &wallet, value, true).await;
        match res {
            Ok(_) => format!("Deposited: {value}"),
            Err(e) => format!("Error: {e}"),
        }
    }
    
    // ストレージに預金
    async fn storage_deposit(self, _: context::Context, amount: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let amount: u128 = amount.replace("_", "").parse().unwrap();
        let res = ref_finance::storage::deposit(&client, &wallet, amount, false).await;
        match res {
            Ok(_) => format!("Deposited: {amount}"),
            Err(e) => format!("Error: {e}"),
        }
    }
    
    // トークンの登録を解除
    async fn storage_unregister_token(self, _: context::Context, token_account: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let token: TokenAccount = token_account.parse().unwrap();
        let res = ref_finance::deposit::unregister_tokens(&client, &wallet, &[token]).await;
        match res {
            Ok(_) => format!("Unregistered: {token_account}"),
            Err(e) => format!("Error: {e}"),
        }
    }
    
    // 預金リストを取得
    async fn deposit_list(self, _: context::Context) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let account = wallet.account_id();
        let res = ref_finance::deposit::get_deposits(&client, account).await;
        match res {
            Err(e) => format!("Error: {e}"),
            Ok(deposits) => {
                let mut lines = Vec::new();
                for (token, amount) in deposits.iter() {
                    let m = MicroNear::from_yocto(amount.0);
                    let line = format!("{token} -> {m:?}");
                    lines.push(line);
                }
                lines.sort();
                lines.join("\n")
            }
        }
    }
    
    // ネイティブトークンをラップ
    async fn wrap_native_token(self, _: context::Context, amount: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
        let amount = MicroNear::of(amount_micro).to_yocto();
        let account = wallet.account_id();
        let before = ref_finance::deposit::wnear::balance_of(&client, account)
            .await
            .unwrap();
        let call = async {
            ref_finance::deposit::wnear::wrap(&client, &wallet, amount)
                .await?
                .wait_for_success()
                .await
        };
        match call.await {
            Ok(_) => {
                let after = ref_finance::deposit::wnear::balance_of(&client, account)
                    .await
                    .unwrap();
                format!("Wrapped: {amount}\n{before}\n{after}")
            }
            Err(err) => format!("Error: {err}"),
        }
    }
    
    // ネイティブトークンをアンラップ
    async fn unwrap_native_token(self, _: context::Context, amount: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
        let amount = MicroNear::of(amount_micro).to_yocto();
        let account = wallet.account_id();
        let before = ref_finance::deposit::wnear::balance_of(&client, account)
            .await
            .unwrap();
        let call = async {
            ref_finance::deposit::wnear::unwrap(&client, &wallet, amount)
                .await?
                .wait_for_success()
                .await
        };
        match call.await {
            Ok(_) => {
                let after = ref_finance::deposit::wnear::balance_of(&client, account)
                    .await
                    .unwrap();
                format!("Unwrapped: {amount}\n{before}\n{after}")
            }
            Err(err) => format!("Error: {err}"),
        }
    }
    
    // トークンを預金
    async fn deposit_token(self, _: context::Context, token_account: String, amount: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
        let amount = MicroNear::of(amount_micro).to_yocto();
        let token = token_account.parse().unwrap();
        let res = ref_finance::deposit::deposit(&client, &wallet, &token, amount).await;
        match res {
            Ok(_) => format!("Deposited: {amount}"),
            Err(e) => format!("Error: {e}"),
        }
    }
    
    // トークンを引き出し
    async fn withdraw_token(self, _: context::Context, token_account: String, amount: String) -> String {
        let client = jsonrpc::new_client();
        let wallet = wallet::new_wallet();
        let amount_micro: u64 = amount.replace("_", "").parse().unwrap();
        let amount = MicroNear::of(amount_micro).to_yocto();
        let token = token_account.parse().unwrap();
        let res = ref_finance::deposit::withdraw(&client, &wallet, &token, amount).await;
        match res {
            Ok(_) => format!("Withdrawn: {amount}"),
            Err(e) => format!("Error: {e}"),
        }
    }
}