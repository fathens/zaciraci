use crate::Result;
use crate::recorder::TradeRecorder;
use crate::slippage::{self, SlippagePolicy};
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
pub async fn calculate_total_portfolio_value(
    current_balances: &BTreeMap<TokenAccount, TokenAmount>,
) -> Result<NearValue> {
    crate::valuation::calculate_portfolio_value(
        current_balances,
        &crate::valuation::LatestRateProvider,
    )
    .await
}

/// execute_direct_swap のパラメータ
pub struct SwapParams<'a> {
    pub from_token: &'a common::types::TokenInAccount,
    pub to_token: &'a common::types::TokenOutAccount,
    pub swap_amount: Option<u128>,
    pub recorder: &'a TradeRecorder,
    pub policy: &'a SlippagePolicy,
}

/// 2つのトークン間で直接スワップを実行（シンプルなパス探索を使用）
///
/// `params.policy` でスリッページ保護の方針を指定する:
/// - `FromExpectedReturn`: 予測リターンに基づく min_out を設定
/// - `Unprotected`: min_out = 0（清算・売却フェーズ用）
pub async fn execute_direct_swap<C, W>(
    client: &C,
    wallet: &W,
    params: &SwapParams<'_>,
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
    let from_token = params.from_token;
    let to_token = params.to_token;
    let policy = params.policy;

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
    let swap_amount_token = params.swap_amount.map(NearToken::from_yoctonear);
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
    let swap_amount = params
        .swap_amount
        .unwrap_or(balance_yocto)
        .min(balance_yocto);

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

    // シンプルなパス探索（利益を考慮しない）
    let path = graph.get_path(&start, &goal)?;
    path.validate_length()?;

    // パスに含まれるすべてのトークン（中継トークン含む）のストレージデポジットを確認
    let tokens = path.all_tokens();
    // keep: 単発スワップでは基軸通貨の WNEAR のみ保持
    let keep = blockchain::ref_finance::storage::keep_wnear_only();
    let max_top_up = near_sdk::NearToken::from_yoctonear(cfg.ref_storage_max_top_up_yoctonear());
    blockchain::ref_finance::storage::ensure_ref_storage_setup(
        client, wallet, &tokens, &keep, max_top_up,
    )
    .await?;

    // AMM 理論出力を事前計算し、スリッページポリシーに基づいて min_out を算出
    let estimated_output = path.calc_value(swap_amount)?;
    let min_out = slippage::calculate_min_out(estimated_output, policy)?;

    debug!(log, "slippage protection";
        "policy" => %policy,
        "estimated_output" => estimated_output,
        "min_out" => min_out,
    );

    let arg = blockchain::ref_finance::swap::SwapArg {
        initial_in: swap_amount,
        min_out,
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

    params
        .recorder
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
