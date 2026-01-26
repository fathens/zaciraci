mod sort;

use super::AppState;
use crate::jsonrpc::{GasInfo, SentTx};
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::path::graph::TokenGraph;
use crate::ref_finance::pool_info::TokenPairLike;
use crate::ref_finance::pool_info::{PoolInfo, PoolInfoList};
use crate::ref_finance::token_account::{
    TokenAccount, TokenInAccount, TokenOutAccount, WNEAR_TOKEN,
};
use crate::types::{MicroNear, MilliNear};
use crate::web::pools::sort::{sort, tokens_with_depth};
use crate::{jsonrpc, ref_finance, wallet};
use axum::Json;
use axum::http::StatusCode;
use axum::{
    Router,
    extract::{Path, State},
    routing::{get, post},
};
use bigdecimal::BigDecimal;
use near_sdk::NearToken;
use num_rational::Ratio;
use num_traits::{ToPrimitive, Zero};
use std::ops::Deref;
use std::sync::Arc;
use std::time::Instant;
use zaciraci_common::ApiResponse;
use zaciraci_common::pools::{
    PoolRecordsRequest, PoolRecordsResponse, SortPoolsRequest, SortPoolsResponse, TradeRequest,
    TradeResponse, VolatilityTokensRequest, VolatilityTokensResponse,
};
use zaciraci_common::types::YoctoNearToken;

const ONE_NEAR: u128 = NearToken::from_near(1).as_yoctonear();

fn path(sub: &str) -> String {
    format!("/pools/{sub}")
}

pub fn add_route(app: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
    app.route(&path("get_all"), get(get_all_pools))
        .route(
            &path("estimate_return/{pool_id}/{amount}"),
            get(estimate_return),
        )
        .route(&path("get_return/{pool_id}/{amount}"), get(get_return))
        .route(&path("list_all_tokens"), get(list_all_tokens))
        .route(
            &path("list_returns/{token_account}/{amount}"),
            get(list_returns),
        )
        .route(
            &path("pick_goals/{token_account}/{amount}"),
            get(pick_goals),
        )
        .route(
            &path("run_swap/{token_in_account}/{initial_value}/{token_out_account}"),
            get(run_swap),
        )
        .route(&path("estimate_trade"), post(estimate_trade))
        .route(&path("get_pool_records"), post(get_pool_records))
        .route(&path("sort_pools"), post(sort_pools))
        .route(&path("get_volatility_tokens"), post(get_volatility_tokens))
}

async fn get_all_pools(
    State(_): State<Arc<AppState>>,
) -> std::result::Result<String, (StatusCode, String)> {
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    Ok(format!("Pools: {}", pools.len()))
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> std::result::Result<String, (StatusCode, String)> {
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let pool = pools.get(pool_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("pool {pool_id} not found: {e}"),
        )
    })?;
    let n = pool.len();
    if n <= 1 {
        return Err((
            StatusCode::BAD_REQUEST,
            crate::ref_finance::errors::Error::InvalidPoolSize(n).to_string(),
        ));
    }
    let token_in = 0;
    let token_out = n - 1;
    let amount_in = amount;
    let pair = pool
        .get_pair(token_in.into(), token_out.into())
        .map_err(|e| (StatusCode::NOT_FOUND, format!("token pair not found: {e}")))?;
    let amount_out = pair.estimate_return(amount_in).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("estimate error: {e}"),
        )
    })?;
    let token_a = pair.token_in_id();
    let token_b = pair.token_out_id();
    Ok(format!(
        "Estimated: {token_a}({amount_in}) -> {token_b}({amount_out})"
    ))
}

async fn get_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> std::result::Result<String, (StatusCode, String)> {
    let client = jsonrpc::new_client();
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let pool = pools.get(pool_id).map_err(|e| {
        (
            StatusCode::NOT_FOUND,
            format!("pool {pool_id} not found: {e}"),
        )
    })?;
    let n = pool.len();
    if n <= 1 {
        return Err((
            StatusCode::BAD_REQUEST,
            crate::ref_finance::errors::Error::InvalidPoolSize(n).to_string(),
        ));
    }
    let token_in = 0;
    let token_out = n - 1;
    let amount_in = amount;
    let pair = pool
        .get_pair(token_in.into(), token_out.into())
        .map_err(|e| (StatusCode::NOT_FOUND, format!("token pair not found: {e}")))?;
    let token_a = pair.token_in_id();
    let token_b = pair.token_out_id();
    let amount_out = pair.get_return(&client, amount_in).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("get_return error: {e}"),
        )
    })?;
    Ok(format!(
        "Return: {token_a}({amount_in}) -> {token_b}({amount_out})"
    ))
}

async fn list_all_tokens(
    State(_): State<Arc<AppState>>,
) -> std::result::Result<String, (StatusCode, String)> {
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let tokens = ref_finance::path::all_tokens(pools);
    let mut tokens: Vec<_> = tokens.iter().map(|t| t.to_string()).collect();
    tokens.sort();
    let mut result = String::from("Tokens:\n");
    for token in tokens {
        result.push_str(&format!("{token}\n"));
    }
    Ok(result)
}

async fn list_returns(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> std::result::Result<String, (StatusCode, String)> {
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let graph = TokenGraph::new(pools);
    let parsed_value: u32 = initial_value
        .replace("_", "")
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid amount: {e}")))?;
    let amount_in = MilliNear::of(parsed_value);
    let start: TokenAccount = token_account.parse().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid token account: {e}"),
        )
    })?;
    let mut sorted_returns = ref_finance::path::sorted_returns(&graph, &start.into(), amount_in)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("sorted_returns error: {e}"),
            )
        })?;
    sorted_returns.reverse();

    let mut result = String::from("from: {token_account}\n");
    for (goal, value, depth) in sorted_returns {
        let rational = Ratio::new(value.to_yocto(), amount_in.to_yocto());
        let ret = rational.to_f32().unwrap_or(f32::NAN);
        result.push_str(&format!("{goal}: {ret}({depth})\n"));
    }
    Ok(result)
}

async fn pick_goals(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> std::result::Result<String, (StatusCode, String)> {
    let gas_price = jsonrpc::new_client()
        .get_gas_price(None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("gas price error: {e}"),
            )
        })?;
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let graph = TokenGraph::new(pools);
    let amount_in: u32 = initial_value
        .replace("_", "")
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid amount: {e}")))?;
    let start: TokenAccount = token_account.parse().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid token account: {e}"),
        )
    })?;
    let goals =
        ref_finance::path::pick_goals(&graph, &start.into(), MilliNear::of(amount_in), gas_price)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("pick_goals error: {e}"),
                )
            })?;
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
    Ok(result)
}

async fn run_swap(
    State(_): State<Arc<AppState>>,
    Path((token_in_account, initial_value, token_out_account)): Path<(String, String, String)>,
) -> std::result::Result<String, (StatusCode, String)> {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let pools = PoolInfoList::read_from_db(None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("DB error: {e}")))?;
    let graph = TokenGraph::new(pools);
    let amount_in: u128 = initial_value
        .replace("_", "")
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid amount: {e}")))?;
    let start_token: TokenAccount = token_in_account
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid token_in: {e}")))?;
    let goal_token: TokenAccount = token_out_account
        .parse()
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid token_out: {e}")))?;
    let start = &start_token.into();
    let goal = &goal_token.into();

    let path = ref_finance::path::swap_path(&graph, start, goal)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("swap_path error: {e}"),
            )
        })?;
    let tokens = ref_finance::swap::gather_token_accounts(&[&path.0]);
    let res = ref_finance::storage::check_and_deposit(&client, &wallet, &tokens)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("storage error: {e}"),
            )
        })?;
    if res.is_some() {
        return Ok("no account to deposit".to_string());
    }

    let arg = ref_finance::swap::SwapArg {
        initial_in: amount_in,
        min_out: amount_in + MilliNear::of(1).to_yocto(),
    };
    let res = ref_finance::swap::run_swap(&client, &wallet, &path.0, arg).await;

    match res {
        Ok((tx_hash, value)) => {
            let outcome = tx_hash
                .wait_for_success()
                .await
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("tx error: {e}")))?;
            Ok(format!("Result: {value:?} ({outcome:?})"))
        }
        Err(e) => Ok(format!("Error: {e}")),
    }
}

async fn estimate_trade(
    State(_): State<Arc<AppState>>,
    Json(request): Json<TradeRequest>,
) -> Json<ApiResponse<TradeResponse, String>> {
    let log = DEFAULT.new(o!(
        "function" => "estimate_trade",
    ));

    let timestamp = request.timestamp;
    let amount_in = request.amount_in.as_yoctonear();
    let start: TokenInAccount = request.token_in.into();
    let goal: TokenOutAccount = request.token_out.into();
    info!(log, "start";
        "timestamp" => %timestamp,
        "amount_in" => %amount_in,
        "start_token" => %start,
        "goal_token" => %goal,
    );

    let pools = match PoolInfoList::read_from_db(Some(timestamp)).await {
        Ok(pools) => pools,
        Err(e) => return Json(ApiResponse::Error(format!("DB error: {e}"))),
    };
    fn out_amount(
        iteration_id: usize,
        prev_out: u128,
        pools: Arc<PoolInfoList>,
        amount_in: u128,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> std::result::Result<u128, String> {
        let log = DEFAULT.new(o!(
            "function" => "estimate_trade::out_amount",
            "iteration_id" => iteration_id,
            "prev_out" => format!("{prev_out}"),
            "amount_in" => format!("{amount_in}"),
            "start_token" => format!("{start}"),
            "goal_token" => format!("{goal}"),
        ));
        let graph = TokenGraph::new(pools);
        if !graph
            .update_single_path(start, goal)
            .map_err(|e| format!("update_single_path error: {e}"))?
        {
            return Err("goal not found".to_string());
        }

        let path = graph
            .get_path(start, goal)
            .map_err(|e| format!("path not found: {e}"))?;
        if log.is_info_enabled() {
            let mut path_info = String::new();
            for token_pair in path.0.iter() {
                let line = format!(
                    "{}: {} -> {}\n",
                    token_pair.pool_id(),
                    token_pair.token_in_id(),
                    token_pair.token_out_id(),
                );
                path_info.push_str(&line);
            }
            info!(log, "path found";
                "path" => %path_info,
            );
        }
        let amount_out = path
            .calc_value(amount_in)
            .map_err(|e| format!("calc_value error: {e}"))?;
        info!(log, "value calculated";
            "amount_out" => %amount_out,
        );
        if prev_out > 0 {
            let reversed_path = path.reversed();
            let reversed_prev_out = reversed_path
                .calc_value(prev_out)
                .map_err(|e| format!("reversed calc_value error: {e}"))?;
            let reversed_amount_out = reversed_path
                .calc_value(amount_out)
                .map_err(|e| format!("reversed calc_value error: {e}"))?;
            info!(log, "reversed value calculated";
                "prev_out" => %prev_out,
                "amount_out" => %amount_out,
                "reversed_prev_out" => %reversed_prev_out,
                "reversed_amount_out" => %reversed_amount_out,
            );
        }
        Ok(amount_out)
    }
    let mut amount_outs: Vec<u128> = vec![];
    for i in 0..10 {
        let prev_out = if i > 0 { amount_outs[i - 1] } else { 0 };
        match out_amount(i, prev_out, pools.clone(), amount_in, &start, &goal) {
            Ok(v) => amount_outs.push(v),
            Err(e) => return Json(ApiResponse::Error(e)),
        }
    }
    let amount_out = match amount_outs.iter().max() {
        Some(v) => *v,
        None => return Json(ApiResponse::Error("no results computed".to_string())),
    };

    Json(ApiResponse::Success(TradeResponse {
        amount_out: YoctoNearToken::from_yocto(amount_out),
    }))
}

async fn get_pool_records(
    State(_): State<Arc<AppState>>,
    Json(request): Json<PoolRecordsRequest>,
) -> Json<ApiResponse<PoolRecordsResponse, String>> {
    let log = DEFAULT.new(o!(
        "function" => "get_pool_records",
        "timestamp" => format!("{}", request.timestamp),
        "pool_ids_count" => request.pool_ids.len(),
    ));
    info!(log, "start");

    let mut pools = vec![];
    // 重複を排除
    let mut pool_ids = request.pool_ids;
    pool_ids.sort();
    pool_ids.dedup();
    for pool_id in pool_ids {
        let res = PoolInfo::get_latest_before(pool_id.into(), request.timestamp).await;
        match res {
            Ok(Some(pool)) => pools.push(pool.into()),
            Ok(None) => {
                info!(log, "pool not found"; "pool_id" => %pool_id.0);
            }
            Err(e) => {
                error!(log, "failed to get pool";
                    "pool_id" => %pool_id.0,
                    "error" => ?e,
                );
                return Json(ApiResponse::Error(e.to_string()));
            }
        }
    }
    info!(log, "finished");
    Json(ApiResponse::Success(PoolRecordsResponse { pools }))
}

async fn sort_pools(
    State(_): State<Arc<AppState>>,
    Json(request): Json<SortPoolsRequest>,
) -> Json<ApiResponse<SortPoolsResponse, String>> {
    let log = DEFAULT.new(o!(
        "function" => "sort_pools",
        "timestamp" => format!("{}", request.timestamp),
        "limit" => request.limit,
    ));
    info!(log, "start");

    let pools = match PoolInfoList::read_from_db(Some(request.timestamp)).await {
        Ok(pools) => pools,
        Err(e) => {
            error!(log, "failed to read pools from DB"; "error" => ?e);
            return Json(ApiResponse::Error(format!("DB error: {e}")));
        }
    };
    let sorted = match sort(pools) {
        Ok(sorted) => sorted
            .iter()
            .take(request.limit as usize)
            .map(|src| src.deref().clone().into())
            .collect(),
        Err(e) => {
            error!(log, "failed to sort pools";
                "error" => ?e,
            );
            return Json(ApiResponse::Error(e.to_string()));
        }
    };
    let res = SortPoolsResponse { pools: sorted };
    info!(log, "finished");
    Json(ApiResponse::Success(res))
}

async fn get_volatility_tokens(
    State(_): State<Arc<AppState>>,
    Json(request): Json<VolatilityTokensRequest>,
) -> Json<ApiResponse<VolatilityTokensResponse, String>> {
    let min_depth = BigDecimal::from(request.min_depth.unwrap_or(1_000_000));
    let log = DEFAULT.new(o!(
        "function" => "volatility_tokens",
        "range.start" => format!("{}", request.start),
        "range.end" => format!("{}", request.end),
        "limit" => format!("{}", request.limit),
        "min_depth" => format!("{}", min_depth),
    ));
    info!(log, "start");

    let quote: TokenInAccount = if let Some(quote_token_str) = &request.quote_token {
        match quote_token_str.parse::<TokenAccount>() {
            Ok(token) => token.into(),
            Err(e) => {
                warn!(log, "failed to parse quote token";
                    "token" => quote_token_str,
                    "error" => ?e,
                );
                WNEAR_TOKEN.to_in()
            }
        }
    } else {
        WNEAR_TOKEN.to_in()
    };
    info!(log, "using quote token";
        "token" => format!("{}", quote),
    );

    // 並行処理でボラティリティ計算と深度計算を同時実行
    let (vols, deps) = {
        let start_time = Instant::now();

        // ボラティリティ計算タスク
        let vol_task = {
            let range = TimeRange {
                start: request.start,
                end: request.end,
            };
            let log = log.clone();
            let quote = quote.clone();
            tokio::spawn(async move {
                info!(log, "start volatility calculation";
                    "quote" => format!("{}", &quote),
                );
                let result = TokenRate::get_by_volatility_in_time_range(&range, &quote).await;
                info!(log, "volatility calculation completed");
                result
            })
        };

        // 深度計算タスク
        let depth_task = {
            let timestamp = request.end;
            let log = log.clone();
            tokio::spawn(async move {
                info!(log, "start depth calculation");
                let result = PoolInfoList::read_from_db(Some(timestamp))
                    .await
                    .and_then(|pools| tokens_with_depth(pools, (&quote, ONE_NEAR)));
                info!(log, "depth calculation completed");
                result
            })
        };

        // 両方の結果を待機
        let (vol_result, depth_result) = tokio::join!(vol_task, depth_task);

        let vols = match vol_result {
            Ok(Ok(vols)) => vols,
            Ok(Err(e)) => {
                error!(log, "failed to get volatility"; "error" => ?e);
                return Json(ApiResponse::Error(e.to_string()));
            }
            Err(e) => {
                error!(log, "volatility task panicked"; "error" => ?e);
                return Json(ApiResponse::Error(
                    "Volatility calculation failed".to_string(),
                ));
            }
        };

        let deps = match depth_result {
            Ok(Ok(deps)) => deps,
            Ok(Err(e)) => {
                error!(log, "failed to get tokens with depth"; "error" => ?e);
                return Json(ApiResponse::Error(e.to_string()));
            }
            Err(e) => {
                error!(log, "depth task panicked"; "error" => ?e);
                return Json(ApiResponse::Error("Depth calculation failed".to_string()));
            }
        };

        let elapsed = start_time.elapsed();
        info!(log, "parallel computation completed";
            "elapsed_ms" => elapsed.as_millis(),
            "elapsed_secs" => elapsed.as_secs_f64(),
        );

        (vols, deps)
    };

    let tokens = {
        let start_time = Instant::now();
        info!(log, "sort tokens";
            "count" => format!("{}", vols.len()),
        );
        let mut weights: Vec<_> = vols
            .into_iter()
            .filter_map(|v| {
                let token = v.base;
                deps.get(&token)
                    .filter(|depth| *depth >= &min_depth)
                    .map(|depth| {
                        let weight = calculate_volatility_weight(&v.variance, depth);
                        (token, weight)
                    })
            })
            .collect();
        weights.sort_by(|(_, aw), (_, bw)| bw.cmp(aw));

        let result = weights
            .into_iter()
            .take(request.limit as usize)
            .map(|(token, _)| token)
            .collect();

        let elapsed = start_time.elapsed();
        info!(log, "tokens part completed";
            "elapsed_ms" => elapsed.as_millis(),
            "elapsed_secs" => elapsed.as_secs_f64(),
        );
        result
    };

    let res = VolatilityTokensResponse { tokens };
    info!(log, "finished");
    Json(ApiResponse::Success(res))
}

/// ボラティリティスコアを計算する関数
/// variance（分散）とdepth（深度）からweight（重み）を算出
///
/// # 引数
/// * `variance` - トークンの価格分散（NEAR²単位）
/// * `depth` - プールの流動性深度（NEAR単位）
///
/// # 戻り値
/// * ボラティリティスコア（NEAR単位）
///
/// # 計算式
/// weight = sqrt(variance) * ln(depth + 1)
/// - variance（NEAR²）→ std_dev（NEAR）で単位統一
/// - 対数変換により大きな深度の影響を適切にスケール調整
fn calculate_volatility_weight(variance: &BigDecimal, depth: &BigDecimal) -> BigDecimal {
    // 標準偏差ベースの正規化: variance(NEAR²) -> std_dev(NEAR)
    let std_dev = variance.sqrt().unwrap_or_else(BigDecimal::zero);

    // 対数変換で深度の影響を調整
    let log_depth = calculate_log_depth(depth);

    // 最終的な重み計算
    std_dev * log_depth
}

/// 深度値の対数を計算する改善された関数
/// 精度損失、範囲制限、パフォーマンスを考慮した実装
fn calculate_log_depth(depth: &BigDecimal) -> BigDecimal {
    // 非正値のチェック
    if *depth <= BigDecimal::zero() {
        return BigDecimal::zero(); // ln(0 + 1) = ln(1) = 0
    }

    let depth_plus_one = depth + BigDecimal::from(1);

    // BigDecimalが非常に大きい場合のハンドリング
    if let Some(depth_f64) = depth_plus_one.to_f64() {
        if depth_f64.is_finite() && depth_f64 > 0.0 {
            // f64範囲内: 通常の対数計算
            let log_value = depth_f64.ln();
            // 小数点以下6桁に制限して精度をコントロール
            let rounded_log = (log_value * 1_000_000.0).round() / 1_000_000.0;
            let mut result =
                BigDecimal::try_from(rounded_log).unwrap_or_else(|_| BigDecimal::zero());
            // BigDecimalの精度を6桁に制限
            result = result.round(6);
            result
        } else {
            // 無限大やNaNの場合: 大きな固定値を使用
            BigDecimal::from(200) // ln(1e86) ≈ 200程度
        }
    } else {
        // BigDecimalがf64範囲を超える場合: 推定値を使用
        // ln(depth + 1) ≈ ln(depth) for very large depth
        // depth ≈ 10^n の場合、ln(depth) ≈ n * ln(10) ≈ n * 2.3
        let depth_str = depth.to_string();
        if let Some(e_pos) = depth_str.find('e') {
            // 科学記法の場合
            if let Ok(exponent) = depth_str[e_pos + 1..].parse::<i32>() {
                let estimated_log = BigDecimal::try_from(exponent as f64 * std::f64::consts::LN_10)
                    .unwrap_or_else(|_| BigDecimal::from(200));
                return estimated_log.min(BigDecimal::from(200));
            }
        }

        // その他の極大値: 固定の大きな値
        BigDecimal::from(200)
    }
}

#[cfg(test)]
mod tests;
