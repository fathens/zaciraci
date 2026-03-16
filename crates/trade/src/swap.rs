use crate::Result;
use crate::recorder::TradeRecorder;
use bigdecimal::BigDecimal;
use blockchain::jsonrpc::SentTx;
use common::types::{NearValue, TokenAccount, TokenAmount};
use logging::*;
use near_sdk::NearToken;
use std::collections::BTreeMap;

/// ポートフォリオ全体の現在残高を取得（TokenAmount: smallest_units + decimals）
pub async fn get_current_portfolio_balances<C, W>(
    client: &C,
    wallet: &W,
    tokens: &[TokenAccount],
) -> Result<BTreeMap<TokenAccount, TokenAmount>>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    W: blockchain::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "get_current_portfolio_balances"));
    let mut balances = BTreeMap::new();

    // REF Finance の全デポジット残高を一度に取得（refillをトリガーしない）
    let account = wallet.account_id();
    let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;

    for token in tokens {
        // depositsから該当トークンの残高を取得
        let balance = deposits.get(token).map(|u| u.0).unwrap_or_default();

        // トークンの decimals を取得
        let decimals = crate::token_cache::get_token_decimals_cached(client, token).await?;

        balances.insert(
            token.clone(),
            TokenAmount::from_smallest_units(BigDecimal::from(balance), decimals),
        );

        trace!(log, "retrieved balance"; "token" => %token, "balance" => balance, "decimals" => decimals);
    }

    Ok(balances)
}

/// ポートフォリオの総価値を計算（NEAR単位）
pub async fn calculate_total_portfolio_value<C, W>(
    _client: &C,
    _wallet: &W,
    current_balances: &BTreeMap<TokenAccount, TokenAmount>,
) -> Result<NearValue>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    W: blockchain::wallet::Wallet,
{
    use common::types::ExchangeRate;

    let log = DEFAULT.new(o!("function" => "calculate_total_portfolio_value"));
    let mut total_value = NearValue::zero();

    let wnear_token = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;

    for (token, amount) in current_balances {
        if amount.is_zero() {
            continue;
        }

        // wrap.nearの場合はそのまま価値とする（decimals=24）
        if token == wnear_token {
            // wrap.near: 1 NEAR = 1 wNEAR (固定レート)
            let rate = ExchangeRate::wnear();
            let value = amount / &rate;
            total_value = total_value + value;
        } else {
            // 他のトークンの場合は、wrap.nearとの交換レートを使用して価値を計算
            use common::types::TokenOutAccount;
            use persistence::token_rate::TokenRate;

            let base_token: TokenOutAccount = token.clone().into();
            let quote_token = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();

            // 最新のレートを取得
            match TokenRate::get_latest(&base_token, &quote_token).await {
                Ok(Some(rate)) => {
                    let spot = rate.to_spot_rate();
                    if spot.is_effectively_zero() {
                        warn!(log, "Rate is effectively zero for token"; "token" => %token);
                    } else {
                        let token_value = amount / &spot;
                        total_value = total_value + token_value;
                    }
                }
                Ok(None) => {
                    warn!(log, "No price data found for token"; "token" => %token);
                }
                Err(e) => {
                    warn!(log, "Failed to get price for token"; "token" => %token, "error" => %e);
                }
            }
        }
    }

    trace!(log, "calculated total portfolio value"; "total_value" => %total_value);
    Ok(total_value)
}

/// 2つのトークン間で直接スワップを実行（シンプルなパス探索を使用）
pub async fn execute_direct_swap<C, W>(
    client: &C,
    wallet: &W,
    from_token: &common::types::TokenInAccount,
    to_token: &common::types::TokenOutAccount,
    swap_amount: Option<u128>,
    recorder: &TradeRecorder,
    cfg: &impl common::config::ConfigAccess,
) -> Result<()>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    <C as blockchain::jsonrpc::SendTx>::Output: std::fmt::Display + blockchain::jsonrpc::SentTx,
    W: blockchain::wallet::Wallet,
{
    let log = DEFAULT.new(o!(
        "function" => "execute_direct_swap",
        "from" => format!("{}", from_token),
        "to" => format!("{}", to_token)
    ));
    debug!(log, "starting direct swap");

    // 型安全な TokenAccount に変換
    let from_token_account: common::types::TokenAccount = from_token.inner().clone();
    let to_token_account: common::types::TokenAccount = to_token.inner().clone();

    // from_tokenの残高を取得
    // wrap.nearの場合のみ balances::start を使用（refill/harvest処理が必要な場合があるため）
    let swap_amount_token = swap_amount.map(NearToken::from_yoctonear);
    let balance = if from_token_account == *blockchain::ref_finance::token_account::WNEAR_TOKEN {
        blockchain::ref_finance::balances::start(
            client,
            wallet,
            &from_token_account,
            swap_amount_token,
            cfg,
        )
        .await?
    } else {
        // その他のトークンは直接 get_deposits で残高を取得
        let account = wallet.account_id();
        let deposits = blockchain::ref_finance::deposit::get_deposits(client, account).await?;
        let yocto = deposits
            .get(&from_token_account)
            .map(|u| u.0)
            .unwrap_or_default();
        NearToken::from_yoctonear(yocto)
    };

    if balance.as_yoctonear() == 0 {
        return Err(anyhow::anyhow!("No balance for token: {}", from_token));
    }

    // swap_amountが指定されていない場合は残高の全額、指定されている場合は指定金額を使用
    let balance_yocto = balance.as_yoctonear();
    let swap_amount = swap_amount.unwrap_or(balance_yocto).min(balance_yocto);

    // プールデータを読み込み
    let pools = persistence::pool_info::read_from_db(None).await?;
    let graph = blockchain::ref_finance::path::graph::TokenGraph::new(pools);

    // パス検索用のトークンを準備
    let start: common::types::TokenInAccount = from_token_account.clone().into();
    let goal: common::types::TokenOutAccount = to_token_account.clone().into();

    // from_tokenを起点としてグラフを更新（流動性のあるトークンのみ含める）
    graph
        .update_graph(&start)
        .map_err(|e| anyhow::anyhow!("Failed to update graph from {}: {}", from_token, e))?;

    // パスに含まれるトークンのストレージデポジットを確認
    let tokens = vec![from_token_account, to_token_account];

    // シンプルなパス探索（利益を考慮しない）
    let path = graph.get_path(&start, &goal)?;
    let res = blockchain::ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;
    if res.is_none() {
        return Err(anyhow::anyhow!("Failed to deposit storage"));
    }

    // スワップ引数を準備
    // NOTE: min_out: 0 は意図的な設計。スリッページ保護を設けない理由:
    // 1. REF Finance のプールは十分な流動性があり、大きなスリッページは稀
    // 2. 価格変動により取引が失敗するリスクより、確実に約定させることを優先
    // 3. リバランスは次回クーロン実行で再試行されるため、一時的な不利な約定は許容範囲
    // 将来的に大規模取引や流動性の低いプールを扱う場合は、見直しを検討する。
    let arg = blockchain::ref_finance::swap::SwapArg {
        initial_in: swap_amount,
        min_out: 0,
    };

    // スワップを実行
    let (sent_tx, out) =
        blockchain::ref_finance::swap::run_swap(client, wallet, &path.0, arg).await?;

    let outcome = match sent_tx.wait_for_success().await {
        Ok(outcome) => outcome,
        Err(e) => {
            error!(log, "swap transaction failed"; "error" => %e);
            return Err(anyhow::anyhow!("Swap transaction failed: {}", e));
        }
    };

    // トークンの decimals を取得して TokenAmount を作成
    let from_decimals =
        crate::token_cache::get_token_decimals_cached(client, from_token.inner()).await?;
    let to_decimals =
        crate::token_cache::get_token_decimals_cached(client, to_token.inner()).await?;

    // 実績値を抽出
    let actual_to_amount = match blockchain::ref_finance::swap::extract_actual_output(&outcome) {
        Ok(actual) => {
            if actual == 0 {
                warn!(log, "swap returned zero output amount";
                    "from" => %from_token, "to" => %to_token);
            }
            Some(TokenAmount::from_smallest_units(
                BigDecimal::from(actual),
                to_decimals,
            ))
        }
        Err(e) => {
            warn!(log, "failed to extract actual output"; "error" => %e);
            None
        }
    };

    info!(log, "swap successful";
        "from" => %from_token,
        "to" => %to_token,
        "input" => swap_amount,
        "estimated_output" => out,
        "actual_output" => actual_to_amount.as_ref().map(|a| a.to_string()).unwrap_or_else(|| "N/A".to_string()),
    );

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
            actual_to_amount,
        )
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests;
