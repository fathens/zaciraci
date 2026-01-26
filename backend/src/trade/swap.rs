use crate::Result;
use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::trade::recorder::TradeRecorder;
use bigdecimal::BigDecimal;
use std::collections::BTreeMap;
use zaciraci_common::types::{NearValue, TokenAmount};

/// ポートフォリオ全体の現在残高を取得（TokenAmount: smallest_units + decimals）
pub async fn get_current_portfolio_balances<C, W>(
    client: &C,
    wallet: &W,
    tokens: &[String],
) -> Result<BTreeMap<String, TokenAmount>>
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

        // トークンの decimals を取得
        let decimals = crate::trade::token_cache::get_token_decimals_cached(client, token).await?;

        balances.insert(
            token.clone(),
            TokenAmount::from_smallest_units(BigDecimal::from(balance), decimals),
        );

        info!(log, "retrieved balance"; "token" => token, "balance" => balance, "decimals" => decimals);
    }

    Ok(balances)
}

/// ポートフォリオの総価値を計算（NEAR単位）
pub async fn calculate_total_portfolio_value<C, W>(
    _client: &C,
    _wallet: &W,
    current_balances: &BTreeMap<String, TokenAmount>,
) -> Result<NearValue>
where
    C: crate::jsonrpc::AccountInfo
        + crate::jsonrpc::SendTx
        + crate::jsonrpc::ViewContract
        + crate::jsonrpc::GasInfo,
    W: crate::wallet::Wallet,
{
    use zaciraci_common::types::ExchangeRate;

    let log = DEFAULT.new(o!("function" => "calculate_total_portfolio_value"));
    let mut total_value = NearValue::zero();

    for (token, amount) in current_balances {
        if amount.is_zero() {
            continue;
        }

        // wrap.nearの場合はそのまま価値とする（decimals=24）
        if token == &crate::ref_finance::token_account::WNEAR_TOKEN.to_string() {
            // wrap.near: 1 NEAR = 1 wNEAR (固定レート)
            let rate = ExchangeRate::wnear();
            let value = amount / &rate;
            total_value = total_value + value;
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
                    // TokenAmount / &ExchangeRate = NearValue トレイトを使用
                    // TokenRate は既に正しい ExchangeRate を持っている
                    // decimals は DB backfill 時に設定済み
                    if rate.exchange_rate.is_zero() {
                        warn!(log, "Rate is zero for token"; "token" => token);
                    } else {
                        let token_value = amount / &rate.exchange_rate;
                        total_value = total_value + token_value;
                    }
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
    from_token: &crate::ref_finance::token_account::TokenInAccount,
    to_token: &crate::ref_finance::token_account::TokenOutAccount,
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

    // 型安全な TokenAccount に変換
    let from_token_account: crate::ref_finance::token_account::TokenAccount =
        from_token.inner().clone();
    let to_token_account: crate::ref_finance::token_account::TokenAccount =
        to_token.inner().clone();

    // from_tokenの残高を取得
    // wrap.nearの場合のみ balances::start を使用（refill/harvest処理が必要な場合があるため）
    let balance = if from_token_account == *crate::ref_finance::token_account::WNEAR_TOKEN {
        crate::ref_finance::balances::start(client, wallet, &from_token_account, swap_amount)
            .await?
    } else {
        // その他のトークンは直接 get_deposits で残高を取得
        let account = wallet.account_id();
        let deposits = crate::ref_finance::deposit::get_deposits(client, account).await?;
        deposits
            .get(&from_token_account)
            .map(|u| u.0)
            .unwrap_or_default()
    };

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
        "from" => %from_token,
        "to" => %to_token,
        "input" => swap_amount,
        "output" => out,
    );

    // トレード記録を保存
    // トークンの decimals を取得して TokenAmount を作成
    let from_decimals =
        crate::trade::token_cache::get_token_decimals_cached(client, &from_token.to_string())
            .await?;
    let to_decimals =
        crate::trade::token_cache::get_token_decimals_cached(client, &to_token.to_string()).await?;
    let from_amount =
        TokenAmount::from_smallest_units(BigDecimal::from(swap_amount), from_decimals);
    let to_amount = TokenAmount::from_smallest_units(BigDecimal::from(out), to_decimals);

    recorder
        .record_trade(
            sent_tx.to_string(),
            from_token,
            from_amount,
            to_token,
            to_amount,
        )
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests;
