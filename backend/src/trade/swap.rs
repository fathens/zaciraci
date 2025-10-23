use crate::Result;
use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::trade::recorder::TradeRecorder;
use bigdecimal::BigDecimal;
use num_traits::Zero;
use std::collections::BTreeMap;
use std::str::FromStr;
use zaciraci_common::algorithm::types::TradingAction;

/// TradingActionに基づいて単一のアクションを実行する
#[allow(dead_code)]
pub async fn execute_single_action<C, W>(
    client: &C,
    wallet: &W,
    action: &TradingAction,
    recorder: &TradeRecorder,
) -> Result<()>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    <C as crate::jsonrpc::SendTx>::Output: std::fmt::Display,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "execute_single_action"));

    match action {
        TradingAction::Hold => {
            // HODLなので何もしない
            info!(log, "holding position");
            Ok(())
        }
        TradingAction::Sell { token, target } => {
            // token を売却して target を購入
            info!(log, "executing sell"; "from" => token, "to" => target);

            // 2段階のswap: token → wrap.near → target
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;

            // Step 1: token → wrap.near
            if token != &wrap_near.to_string() {
                execute_direct_swap(
                    client,
                    wallet,
                    token,
                    &wrap_near.to_string(),
                    None,
                    recorder,
                )
                .await?;
            }

            // Step 2: wrap.near → target
            if target != &wrap_near.to_string() {
                execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near.to_string(),
                    target,
                    None,
                    recorder,
                )
                .await?;
            }

            info!(log, "sell completed"; "from" => token, "to" => target);
            Ok(())
        }
        TradingAction::Switch { from, to } => {
            // from から to へ切り替え（直接スワップ）
            info!(log, "executing switch"; "from" => from, "to" => to);
            execute_direct_swap(client, wallet, from, to, None, recorder).await?;
            info!(log, "switch completed"; "from" => from, "to" => to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            info!(log, "executing rebalance"; "weights" => ?target_weights);

            // 1. 現在の保有量を取得（wrap.nearを明示的に追加）
            let mut tokens: Vec<String> = target_weights.keys().cloned().collect();
            info!(log, "tokens from target_weights"; "tokens" => ?tokens, "count" => tokens.len());

            let wrap_near = crate::ref_finance::token_account::WNEAR_TOKEN.to_string();
            if !tokens.contains(&wrap_near) {
                tokens.push(wrap_near.clone());
                info!(
                    log,
                    "added wrap.near to balance query for total value calculation"
                );
            }
            info!(log, "tokens list before get_current_portfolio_balances"; "tokens" => ?tokens, "count" => tokens.len());

            let current_balances = get_current_portfolio_balances(client, wallet, &tokens).await?;

            // 2. 総ポートフォリオ価値を計算
            let total_value =
                calculate_total_portfolio_value(client, wallet, &current_balances).await?;

            if total_value == BigDecimal::from(0) {
                return Err(anyhow::anyhow!("Total portfolio value is zero"));
            }

            // 3. 各トークンのリバランス実行
            for (token, target_weight) in target_weights.iter() {
                let current_balance = current_balances.get(token).copied().unwrap_or(0);

                // 目標量を計算（total_value × target_weight）
                let target_value = &total_value * BigDecimal::from_str(&target_weight.to_string())?;
                let current_value = BigDecimal::from(current_balance);

                // 差分を計算
                let value_diff = &target_value - &current_value;

                info!(log, "rebalancing token";
                    "token" => token,
                    "target_weight" => target_weight,
                    "current_balance" => current_balance,
                    "target_value" => %target_value,
                    "current_value" => %current_value,
                    "value_diff" => %value_diff
                );

                // リスク管理: 最大トレードサイズを総価値の10%に制限
                let max_trade_size = &total_value * BigDecimal::from_str("0.1")?;
                let trade_amount = if value_diff.abs() > max_trade_size {
                    if value_diff > BigDecimal::from(0) {
                        max_trade_size.clone()
                    } else {
                        -max_trade_size.clone()
                    }
                } else {
                    value_diff.clone()
                };

                // 最小トレードサイズのチェック（総価値の1%未満はスキップ）
                let min_trade_size = &total_value * BigDecimal::from_str("0.01")?;
                if trade_amount.abs() < min_trade_size {
                    info!(log, "skipping small rebalance"; "token" => token, "trade_amount" => %trade_amount);
                    continue;
                }

                // 4. スワップ実行
                let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;

                if trade_amount > BigDecimal::from(0) {
                    // ポジション増加: wrap.near → token
                    if token != &wrap_near.to_string() {
                        info!(log, "increasing position"; "token" => token, "amount" => %trade_amount);
                        execute_direct_swap(
                            client,
                            wallet,
                            &wrap_near.to_string(),
                            token,
                            None,
                            recorder,
                        )
                        .await?;
                    }
                } else if trade_amount < BigDecimal::from(0) {
                    // ポジション削減: token → wrap.near
                    if token != &wrap_near.to_string() {
                        info!(log, "reducing position"; "token" => token, "amount" => %trade_amount.abs());
                        execute_direct_swap(
                            client,
                            wallet,
                            token,
                            &wrap_near.to_string(),
                            None,
                            recorder,
                        )
                        .await?;
                    }
                }
            }

            info!(log, "rebalance completed");
            Ok(())
        }
        TradingAction::AddPosition { token, weight } => {
            // ポジション追加
            info!(log, "adding position"; "token" => token, "weight" => weight);

            // wrap.near → token へのswap
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
            if token != &wrap_near.to_string() {
                execute_direct_swap(
                    client,
                    wallet,
                    &wrap_near.to_string(),
                    token,
                    None,
                    recorder,
                )
                .await?;
            }

            info!(log, "position added"; "token" => token, "weight" => weight);
            Ok(())
        }
        TradingAction::ReducePosition { token, weight } => {
            // ポジション削減
            info!(log, "reducing position"; "token" => token, "weight" => weight);

            // token → wrap.near へのswap
            let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
            if token != &wrap_near.to_string() {
                execute_direct_swap(
                    client,
                    wallet,
                    token,
                    &wrap_near.to_string(),
                    None,
                    recorder,
                )
                .await?;
            }

            info!(log, "position reduced"; "token" => token, "weight" => weight);
            Ok(())
        }
    }
}

/// ポートフォリオ全体の現在残高を取得（yoctoNEAR単位）
pub async fn get_current_portfolio_balances<C, W>(
    client: &C,
    wallet: &W,
    tokens: &[String],
) -> Result<BTreeMap<String, u128>>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "get_current_portfolio_balances"));
    let mut balances = BTreeMap::new();

    // REF Finance の全デポジット残高を一度に取得（refillをトリガーしない）
    let account = wallet.account_id();
    let deposits = crate::ref_finance::deposit::get_deposits(client, account).await?;

    for token in tokens {
        let token_account: crate::ref_finance::token_account::TokenAccount = token
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid token: {}", e))?;

        // depositsから該当トークンの残高を取得
        let balance = deposits
            .get(&token_account)
            .map(|u| u.0)
            .unwrap_or_default();
        balances.insert(token.clone(), balance);

        info!(log, "retrieved balance"; "token" => token, "balance" => balance);
    }

    Ok(balances)
}

/// ポートフォリオの総価値を計算（yoctoNEAR単位）
pub async fn calculate_total_portfolio_value<C, W>(
    _client: &C,
    _wallet: &W,
    current_balances: &BTreeMap<String, u128>,
) -> Result<BigDecimal>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "calculate_total_portfolio_value"));
    let mut total_value = BigDecimal::from(0);

    for (token, balance) in current_balances {
        if *balance == 0 {
            continue;
        }

        // wrap.nearの場合はそのまま価値とする
        if token == &crate::ref_finance::token_account::WNEAR_TOKEN.to_string() {
            total_value += BigDecimal::from(*balance);
        } else {
            // 他のトークンの場合は、wrap.nearとの交換レートを使用して価値を計算
            use crate::persistence::token_rate::TokenRate;
            use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
            use near_sdk::AccountId;

            let base_token = match token.parse::<AccountId>() {
                Ok(account_id) => TokenOutAccount::from(account_id),
                Err(_) => {
                    warn!(log, "Invalid token account ID"; "token" => token);
                    continue;
                }
            };
            let quote_token =
                TokenInAccount::from(crate::ref_finance::token_account::WNEAR_TOKEN.clone());

            // 最新のレートを取得
            match TokenRate::get_latest(&base_token, &quote_token).await {
                Ok(Some(rate)) => {
                    // balance * rateで価値を計算
                    let token_value = BigDecimal::from(*balance) * rate.rate;
                    total_value += token_value;
                }
                Ok(None) => {
                    // レートが見つからない場合は警告を出して0として扱う
                    warn!(log, "No price data found for token"; "token" => token);
                }
                Err(e) => {
                    // エラーの場合も警告を出して0として扱う
                    warn!(log, "Failed to get price for token"; "token" => token, "error" => %e);
                }
            }
        }
    }

    info!(log, "calculated total portfolio value"; "total_value" => %total_value);
    Ok(total_value)
}

/// 2つのトークン間で直接スワップを実行（シンプルなパス探索を使用）
pub async fn execute_direct_swap<C, W>(
    client: &C,
    wallet: &W,
    from_token: &str,
    to_token: &str,
    swap_amount: Option<u128>,
    recorder: &TradeRecorder,
) -> Result<()>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    <C as crate::jsonrpc::SendTx>::Output: std::fmt::Display + crate::jsonrpc::SentTx,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "execute_direct_swap",
        "from" => format!("{}", from_token),
        "to" => format!("{}", to_token)
    ));
    info!(log, "starting direct swap");

    // from_token の残高を取得
    let from_token_account: crate::ref_finance::token_account::TokenAccount = from_token
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid from_token: {}", e))?;
    let to_token_account: crate::ref_finance::token_account::TokenAccount = to_token
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid to_token: {}", e))?;

    // swap_amountが指定されている場合は、その金額が使えるようにbalances::startを呼び出す
    let balance =
        crate::ref_finance::balances::start(client, wallet, &from_token_account, swap_amount)
            .await?;

    if balance == 0 {
        return Err(anyhow::anyhow!("No balance for token: {}", from_token));
    }

    // swap_amountが指定されていない場合は残高の全額、指定されている場合は指定金額を使用
    let swap_amount = swap_amount.unwrap_or(balance).min(balance);

    // プールデータを読み込み
    let pools = crate::ref_finance::pool_info::PoolInfoList::read_from_db(None).await?;
    let graph = crate::ref_finance::path::graph::TokenGraph::new(pools);

    // パス検索用のトークンを準備
    let start: crate::ref_finance::token_account::TokenInAccount =
        from_token_account.clone().into();
    let goal: crate::ref_finance::token_account::TokenOutAccount = to_token_account.clone().into();

    // from_tokenを起点としてグラフを更新（流動性のあるトークンのみ含める）
    graph
        .update_graph(&start)
        .map_err(|e| anyhow::anyhow!("Failed to update graph from {}: {}", from_token, e))?;

    // パスに含まれるトークンのストレージデポジットを確認
    let tokens = vec![from_token_account, to_token_account];

    // シンプルなパス探索（利益を考慮しない）
    let path = graph.get_path(&start, &goal)?;
    let res = crate::ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;
    if res.is_none() {
        return Err(anyhow::anyhow!("Failed to deposit storage"));
    }

    // スワップ引数を準備
    let arg = crate::ref_finance::swap::SwapArg {
        initial_in: swap_amount,
        min_out: 0, // トレードでは最小出力は気にしない
    };

    // スワップを実行
    let (sent_tx, out) = crate::ref_finance::swap::run_swap(client, wallet, &path.0, arg).await?;

    if let Err(e) = sent_tx.wait_for_success().await {
        error!(log, "swap transaction failed"; "error" => %e);
        return Err(anyhow::anyhow!("Swap transaction failed: {}", e));
    }

    info!(log, "swap successful";
        "from" => from_token,
        "to" => to_token,
        "input" => swap_amount,
        "output" => out,
    );

    // トレード記録を保存
    let from_amount = BigDecimal::from(swap_amount);
    let to_amount = BigDecimal::from(out);
    let price = if !from_amount.is_zero() {
        to_amount.clone() / from_amount.clone()
    } else {
        BigDecimal::from(0)
    };

    recorder
        .record_trade(
            sent_tx.to_string(),
            from_token.to_string(),
            from_amount,
            to_token.to_string(),
            to_amount,
            price,
        )
        .await?;

    Ok(())
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct SwapContext<'a> {
    from_token: &'a str,
    to_token: &'a str,
    swap_amount: near_primitives::types::Balance,
    recorder: &'a TradeRecorder,
}

/// arbitrage.rsのswap_each関数に基づくswap実行とrecording
#[allow(clippy::useless_conversion)]
#[allow(dead_code)]
async fn execute_swap_with_recording<A, C, W>(
    client: &C,
    wallet: &W,
    preview: crate::ref_finance::path::preview::Preview<A>,
    path: crate::ref_finance::pool_info::TokenPath,
    context: SwapContext<'_>,
) -> Result<()>
where
    A: Into<near_primitives::types::Balance> + Copy,
    C: crate::jsonrpc::SendTx,
    <C as crate::jsonrpc::SendTx>::Output: std::fmt::Display + crate::jsonrpc::SentTx,
    W: crate::wallet::Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "execute_swap_with_recording",
        "path.len" => format!("{}", path.len()),
    ));

    let initial_in: near_primitives::types::Balance = preview.input_value.into();
    let output_value: near_primitives::types::Balance = preview.output_value.into();
    let gain: near_primitives::types::Balance = preview.gain.into();

    let arg = crate::ref_finance::swap::SwapArg {
        initial_in,
        min_out: output_value - gain,
    };

    let (sent_tx, out) = crate::ref_finance::swap::run_swap(client, wallet, &path.0, arg).await?;

    if let Err(e) = sent_tx.wait_for_success().await {
        error!(log, "transaction failed"; "tx" => %sent_tx, "error" => %e);
        return Err(e);
    }

    info!(log, "swap done";
        "estimated_output" => out,
        "tx" => %sent_tx
    );

    // 取引記録をデータベースに保存
    if let Err(e) = record_successful_trade(
        context.recorder,
        sent_tx.to_string(),
        context.from_token,
        context.swap_amount,
        context.to_token,
        out,
    )
    .await
    {
        error!(log, "failed to record trade"; "tx" => %sent_tx, "error" => %e);
        // 記録失敗はスワップの成功には影響しない
    }

    Ok(())
}

/// 成功した取引をデータベースに記録
#[allow(dead_code)]
async fn record_successful_trade(
    recorder: &TradeRecorder,
    tx_hash: String,
    from_token: &str,
    from_amount: u128,
    to_token: &str,
    to_amount: u128,
) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "record_successful_trade"));

    // yoctoNEAR建て価格を計算（簡易版）
    let price_yocto_near = if from_token.contains("wrap.near") || from_token == "near" {
        BigDecimal::from(from_amount)
    } else if to_token.contains("wrap.near") || to_token == "near" {
        BigDecimal::from(to_amount)
    } else {
        // wrap.near以外の場合、from_amountをベースに推定
        BigDecimal::from(from_amount)
    };

    recorder
        .record_trade(
            tx_hash.clone(),
            from_token.to_string(),
            BigDecimal::from(from_amount),
            to_token.to_string(),
            BigDecimal::from(to_amount),
            price_yocto_near,
        )
        .await?;

    info!(log, "trade recorded successfully"; "tx_hash" => %tx_hash);
    Ok(())
}
