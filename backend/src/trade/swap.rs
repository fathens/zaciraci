use crate::Result;
use crate::jsonrpc::SentTx;
use crate::logging::*;
use crate::trade::recorder::TradeRecorder;
use crate::types::MicroNear;
use bigdecimal::BigDecimal;
use futures_util::future::join_all;
use zaciraci_common::algorithm::types::TradingAction;

/// TradingActionに基づいて単一のアクションを実行する
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
                execute_direct_swap(client, wallet, token, &wrap_near.to_string(), recorder)
                    .await?;
            }

            // Step 2: wrap.near → target
            if target != &wrap_near.to_string() {
                execute_direct_swap(client, wallet, &wrap_near.to_string(), target, recorder)
                    .await?;
            }

            info!(log, "sell completed"; "from" => token, "to" => target);
            Ok(())
        }
        TradingAction::Switch { from, to } => {
            // from から to へ切り替え（直接スワップ）
            info!(log, "executing switch"; "from" => from, "to" => to);
            execute_direct_swap(client, wallet, from, to, recorder).await?;
            info!(log, "switch completed"; "from" => from, "to" => to);
            Ok(())
        }
        TradingAction::Rebalance { target_weights } => {
            // ポートフォリオのリバランス
            info!(log, "executing rebalance"; "weights" => ?target_weights);

            // 各トークンの目標ウェイトに基づいてリバランス
            for (token, weight) in target_weights.iter() {
                info!(log, "rebalancing token"; "token" => token, "weight" => weight);
                // TODO: 現在の保有量と目標量を比較してswap量を計算

                // 簡易実装として、少量のswapを実行
                if *weight > 0.0 {
                    // wrap.near → token へのswap（ポジション増加）
                    let wrap_near = &crate::ref_finance::token_account::WNEAR_TOKEN;
                    if token != &wrap_near.to_string() {
                        execute_direct_swap(
                            client,
                            wallet,
                            &wrap_near.to_string(),
                            token,
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
                execute_direct_swap(client, wallet, &wrap_near.to_string(), token, recorder)
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
                execute_direct_swap(client, wallet, token, &wrap_near.to_string(), recorder)
                    .await?;
            }

            info!(log, "position reduced"; "token" => token, "weight" => weight);
            Ok(())
        }
    }
}

/// 2つのトークン間で直接スワップを実行（arbitrage.rs実装パターンを使用）
async fn execute_direct_swap<C, W>(
    client: &C,
    wallet: &W,
    from_token: &str,
    to_token: &str,
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
    info!(log, "starting direct swap using arbitrage pattern");

    // from_token の残高を取得
    let from_token_account: crate::ref_finance::token_account::TokenAccount = from_token
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid from_token: {}", e))?;
    let balance = crate::ref_finance::balances::start(client, wallet, &from_token_account).await?;

    if balance == 0 {
        return Err(anyhow::anyhow!("No balance for token: {}", from_token));
    }

    // 少量のswapを実行（残高の10%程度）
    let swap_amount = balance / 10;

    // プールデータを読み込み
    let pools = crate::ref_finance::pool_info::PoolInfoList::read_from_db(None).await?;
    let graph = crate::ref_finance::path::graph::TokenGraph::new(pools);

    // パス検索用のstart tokenを準備
    let start: &crate::ref_finance::token_account::TokenInAccount = &from_token_account.into();
    let start_balance = MicroNear::from_yocto(swap_amount);

    // ガス価格を取得
    let gas_price = client.get_gas_price(None).await?;

    // パスを検索（arbitrage.rsの実装を使用）
    let previews =
        crate::ref_finance::path::pick_previews(&graph, start, start_balance, gas_price)?;

    if let Some(previews) = previews {
        let (pre_path, tokens) = previews.into_with_path(&graph, start).await?;

        // ストレージデポジットの確認
        let res = crate::ref_finance::storage::check_and_deposit(client, wallet, &tokens).await?;
        if res.is_none() {
            return Err(anyhow::anyhow!("Failed to deposit storage"));
        }

        // arbitrage.rsパターンでスワップを並列実行
        let context = SwapContext {
            from_token,
            to_token,
            swap_amount,
            recorder,
        };
        let swaps = pre_path.into_iter().map(|(preview, path)| {
            execute_swap_with_recording(client, wallet, preview, path, context)
        });
        let results = join_all(swaps).await;

        let success_count = results.iter().filter(|r| r.is_ok()).count();
        info!(log, "swaps completed";
            "success" => format!("{}/{}", success_count, results.len()),
        );

        // 少なくとも1つ成功していれば OK
        if success_count > 0 {
            info!(log, "direct swap successful"; "from" => from_token, "to" => to_token);
        } else {
            return Err(anyhow::anyhow!("All swap attempts failed"));
        }

        info!(log, "direct swap successful"; "from" => from_token, "to" => to_token);
        Ok(())
    } else {
        warn!(log, "no swap path found"; "from" => from_token, "to" => to_token);
        Err(anyhow::anyhow!(
            "No swap path found from {} to {}",
            from_token,
            to_token
        ))
    }
}

#[derive(Clone, Copy)]
struct SwapContext<'a> {
    from_token: &'a str,
    to_token: &'a str,
    swap_amount: near_primitives::types::Balance,
    recorder: &'a TradeRecorder,
}

/// arbitrage.rsのswap_each関数に基づくswap実行とrecording
#[allow(clippy::useless_conversion)]
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
