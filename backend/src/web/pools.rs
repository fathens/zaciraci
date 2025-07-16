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

async fn get_all_pools(State(_): State<Arc<AppState>>) -> String {
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    format!("Pools: {}", pools.len())
}

async fn estimate_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let pools = PoolInfoList::read_from_db(None).await.unwrap();
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

async fn get_return(
    State(_): State<Arc<AppState>>,
    Path((pool_id, amount)): Path<(u32, u128)>,
) -> String {
    use crate::ref_finance::errors::Error;

    let client = jsonrpc::new_client();
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
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

async fn list_all_tokens(State(_): State<Arc<AppState>>) -> String {
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let tokens = ref_finance::path::all_tokens(pools);
    let mut tokens: Vec<_> = tokens.iter().map(|t| t.to_string()).collect();
    tokens.sort();
    let mut result = String::from("Tokens:\n");
    for token in tokens {
        result.push_str(&format!("{token}\n"));
    }
    result
}

async fn list_returns(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> String {
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let graph = TokenGraph::new(pools);
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

async fn pick_goals(
    State(_): State<Arc<AppState>>,
    Path((token_account, initial_value)): Path<(String, String)>,
) -> String {
    let gas_price = jsonrpc::new_client().get_gas_price(None).await.unwrap();
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let graph = TokenGraph::new(pools);
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

async fn run_swap(
    State(_): State<Arc<AppState>>,
    Path((token_in_account, initial_value, token_out_account)): Path<(String, String, String)>,
) -> String {
    let client = jsonrpc::new_client();
    let wallet = wallet::new_wallet();
    let pools = PoolInfoList::read_from_db(None).await.unwrap();
    let graph = TokenGraph::new(pools);
    let amount_in: u128 = initial_value.replace("_", "").parse().unwrap();
    let start_token: TokenAccount = token_in_account.parse().unwrap();
    let goal_token: TokenAccount = token_out_account.parse().unwrap();
    let start = &start_token.into();
    let goal = &goal_token.into();

    let path = ref_finance::path::swap_path(&graph, start, goal)
        .await
        .unwrap();
    let tokens = ref_finance::swap::gather_token_accounts(&[&path.0]);
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
    let res = ref_finance::swap::run_swap(&client, &wallet, &path.0, arg).await;

    match res {
        Ok((tx_hash, value)) => {
            let outcome = tx_hash.wait_for_success().await.unwrap();
            format!("Result: {value:?} ({outcome:?})")
        }
        Err(e) => format!("Error: {e}"),
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
    let start_token: TokenAccount = request.token_in.try_into().unwrap();
    let goal_token: TokenAccount = request.token_out.try_into().unwrap();
    let start = &start_token.into();
    let goal = &goal_token.into();
    info!(log, "start";
        "timestamp" => %timestamp,
        "amount_in" => %amount_in,
        "start_token" => %start,
        "goal_token" => %goal,
    );

    let pools = PoolInfoList::read_from_db(Some(timestamp)).await.unwrap();
    fn out_amount(
        iteration_id: usize,
        prev_out: u128,
        pools: Arc<PoolInfoList>,
        amount_in: u128,
        start: &TokenInAccount,
        goal: &TokenOutAccount,
    ) -> u128 {
        let log = DEFAULT.new(o!(
            "function" => "estimate_trade::out_amount",
            "iteration_id" => iteration_id,
            "prev_out" => format!("{prev_out}"),
            "amount_in" => format!("{amount_in}"),
            "start_token" => format!("{start}"),
            "goal_token" => format!("{goal}"),
        ));
        let graph = TokenGraph::new(pools);
        if !graph.update_single_path(start, goal).unwrap() {
            panic!("goal not found");
        }

        let path = graph.get_path(start, goal).unwrap();
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
        let amount_out = path.calc_value(amount_in).unwrap();
        info!(log, "value calculated";
            "amount_out" => %amount_out,
        );
        if prev_out > 0 {
            let reversed_path = path.reversed();
            let reversed_prev_out = reversed_path.calc_value(prev_out).unwrap();
            let reversed_amount_out = reversed_path.calc_value(amount_out).unwrap();
            info!(log, "reversed value calculated";
                "prev_out" => %prev_out,
                "amount_out" => %amount_out,
                "reversed_prev_out" => %reversed_prev_out,
                "reversed_amount_out" => %reversed_amount_out,
            );
        }
        amount_out
    }
    let mut amount_outs: Vec<u128> = vec![];
    for i in 0..10 {
        let prev_out = if i > 0 { amount_outs[i - 1] } else { 0 };
        let v = out_amount(i, prev_out, pools.clone(), amount_in, start, goal);
        amount_outs.push(v);
    }
    let amount_out = amount_outs.iter().max().unwrap();

    Json(ApiResponse::Success(TradeResponse {
        amount_out: YoctoNearToken::from_yocto(*amount_out),
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

    let pools = PoolInfoList::read_from_db(Some(request.timestamp))
        .await
        .unwrap();
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
                WNEAR_TOKEN.clone().into()
            }
        }
    } else {
        WNEAR_TOKEN.clone().into()
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
            .map(|(token, _)| token.into())
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
mod tests {
    use bigdecimal::BigDecimal;
    use chrono::Utc;
    use num_traits::ToPrimitive;
    use std::str::FromStr;
    use zaciraci_common::pools::{SortPoolsRequest, SortPoolsResponse};

    fn current_log_depth_calculation(depth: &BigDecimal) -> BigDecimal {
        let depth_plus_one = depth + BigDecimal::from(1);

        // 現在の実装
        match depth_plus_one.to_f64() {
            Some(depth_f64) if depth_f64 > 0.0 => {
                BigDecimal::try_from(depth_f64.ln()).unwrap_or_else(|_| BigDecimal::from(0))
            }
            _ => BigDecimal::from(0),
        }
    }

    #[test]
    fn test_sort_pools_request_structure() {
        let request = SortPoolsRequest {
            timestamp: Utc::now().naive_utc(),
            limit: 10,
        };

        assert_eq!(request.limit, 10);
        assert!(request.timestamp <= Utc::now().naive_utc());
    }

    #[test]
    fn test_sort_pools_response_structure() {
        let response = SortPoolsResponse { pools: vec![] };

        assert!(response.pools.is_empty());
    }

    #[test]
    fn test_log_depth_typical_values() {
        let test_cases = vec![
            ("0.1", "小さな深度"),
            ("1.0", "単位深度"),
            ("10.0", "中程度の深度"),
            ("100.0", "大きな深度"),
            ("1000.0", "非常に大きな深度"),
        ];

        for (value_str, description) in test_cases {
            let depth = BigDecimal::from_str(value_str).unwrap();
            let log_depth = current_log_depth_calculation(&depth);

            println!("{}:", description);
            println!("  入力depth: {}", depth);
            println!("  log(depth + 1): {}", log_depth);

            // 負の値にならないことを確認
            assert!(
                log_depth >= BigDecimal::from(0),
                "深度{}で負の対数値が発生: {}",
                depth,
                log_depth
            );
        }
    }

    #[test]
    fn test_log_depth_edge_cases() {
        // ゼロ値 - ln(0 + 1) = ln(1) = 0
        let zero_depth = BigDecimal::from(0);
        let log_zero = current_log_depth_calculation(&zero_depth);
        println!("ゼロ深度: log({} + 1) = {}", zero_depth, log_zero);
        assert_eq!(log_zero, BigDecimal::from(0));

        // 非常に小さな値
        let tiny_depth = BigDecimal::from_str("1e-10").unwrap();
        let log_tiny = current_log_depth_calculation(&tiny_depth);
        println!("極小深度: log({} + 1) = {}", tiny_depth, log_tiny);
        assert!(log_tiny >= BigDecimal::from(0));

        // 非常に大きな値
        let huge_depth = BigDecimal::from_str("1e50").unwrap();
        let log_huge = current_log_depth_calculation(&huge_depth);
        println!("極大深度: log({} + 1) = {}", huge_depth, log_huge);
        assert!(log_huge > BigDecimal::from(0));
    }

    #[test]
    fn test_log_depth_precision_analysis() {
        // 高精度の値での精度損失を確認
        let high_precision = BigDecimal::from_str("123.456789012345678901234567890").unwrap();
        let log_result = current_log_depth_calculation(&high_precision);

        println!("高精度入力: {}", high_precision);
        println!("対数結果: {}", log_result);

        // f64変換での値
        let as_f64 = (&high_precision + BigDecimal::from(1)).to_f64().unwrap();
        println!("f64変換後: {}", as_f64);

        // f64の精度限界付近での動作を確認
        let very_large = BigDecimal::from_str("1e100").unwrap();
        let log_very_large = current_log_depth_calculation(&very_large);
        println!("極大値: {} -> log_depth: {}", very_large, log_very_large);

        // f64の範囲を超える値
        let beyond_f64 = BigDecimal::from_str("1e400").unwrap();
        let log_beyond = current_log_depth_calculation(&beyond_f64);
        println!("f64範囲超過: {} -> log_depth: {}", beyond_f64, log_beyond);

        // ゼロになることを確認（f64変換でinfinityまたはオーバーフロー）
        assert_eq!(log_beyond, BigDecimal::from(0));
    }

    #[test]
    fn test_improved_calculate_log_depth() {
        use super::calculate_log_depth;

        // テストケース: 典型的な値
        let test_cases = vec![
            ("0", "ゼロ"),
            ("0.1", "小さな深度"),
            ("1.0", "単位深度"),
            ("10.0", "中程度の深度"),
            ("100.0", "大きな深度"),
            ("1000.0", "非常に大きな深度"),
            ("1e50", "極大値(f64範囲内)"),
            ("1e400", "超極大値(f64範囲外)"),
        ];

        for (value_str, description) in test_cases {
            let depth = BigDecimal::from_str(value_str).unwrap();
            let log_depth = calculate_log_depth(&depth);

            println!("{}:", description);
            println!("  入力depth: {}", depth);
            println!("  改善版log_depth: {}", log_depth);

            // 負の値にならないことを確認
            assert!(log_depth >= BigDecimal::from(0));

            // 合理的な上限値を確認
            assert!(log_depth <= BigDecimal::from(200));
        }

        // 精度テスト: 小数点以下の桁数が制限されていることを確認
        let precise_depth = BigDecimal::from_str("2.718281828459045").unwrap(); // e
        let log_e_plus_1 = calculate_log_depth(&precise_depth);
        println!("ln(e + 1) = {}", log_e_plus_1);

        // 精度が制限されていることを確認（6桁程度）
        let log_str = log_e_plus_1.to_string();
        let decimal_places = if let Some(pos) = log_str.find('.') {
            log_str.len() - pos - 1
        } else {
            0
        };
        println!("小数点以下桁数: {}", decimal_places);
        assert!(decimal_places <= 15); // f64の精度制限内
    }

    #[test]
    fn test_calculate_volatility_weight() {
        use super::calculate_volatility_weight;

        // 基本的な動作テスト
        println!("=== ボラティリティ重み計算テスト ===");

        // テストケース 1: ゼロ値のハンドリング
        let zero_variance = BigDecimal::from(0);
        let zero_depth = BigDecimal::from(0);
        let weight_zero = calculate_volatility_weight(&zero_variance, &zero_depth);
        println!("ゼロテスト: variance=0, depth=0 → weight={}", weight_zero);
        assert_eq!(weight_zero, BigDecimal::from(0));

        // テストケース 2: 典型的な値での計算
        let test_cases = vec![
            ("0.01", "1.0", "低分散・低深度"),
            ("0.01", "100.0", "低分散・高深度"),
            ("1.0", "1.0", "中分散・低深度"),
            ("1.0", "100.0", "中分散・高深度"),
            ("100.0", "1.0", "高分散・低深度"),
            ("100.0", "100.0", "高分散・高深度"),
        ];

        for (var_str, dep_str, description) in test_cases {
            let variance = BigDecimal::from_str(var_str).unwrap();
            let depth = BigDecimal::from_str(dep_str).unwrap();
            let weight = calculate_volatility_weight(&variance, &depth);

            println!("{}:", description);
            println!(
                "  variance={}, depth={} → weight={}",
                variance, depth, weight
            );

            // 基本的な制約
            assert!(weight >= BigDecimal::from(0)); // 非負

            // 高分散・高深度の組み合わせで最大値になることを確認
            if var_str == "100.0" && dep_str == "100.0" {
                // これが最も高いスコアになるはず
                assert!(weight > BigDecimal::from(10));
            }
        }
    }

    #[test]
    fn test_volatility_weight_mathematical_properties() {
        use super::calculate_volatility_weight;

        println!("=== 数学的性質のテスト ===");

        // 単調性テスト: 分散が増加すると重みも増加する
        let depth_fixed = BigDecimal::from_str("10.0").unwrap();
        let variances = vec!["0.1", "1.0", "10.0", "100.0"];
        let mut prev_weight = BigDecimal::from(0);

        for var_str in variances {
            let variance = BigDecimal::from_str(var_str).unwrap();
            let weight = calculate_volatility_weight(&variance, &depth_fixed);

            println!("分散単調性: variance={} → weight={}", variance, weight);
            assert!(weight >= prev_weight, "分散の増加で重みが減少しています");
            prev_weight = weight;
        }

        // 単調性テスト: 深度が増加すると重みも増加する
        let variance_fixed = BigDecimal::from_str("1.0").unwrap();
        let depths = vec!["0.1", "1.0", "10.0", "100.0"];
        let mut prev_weight = BigDecimal::from(0);

        for dep_str in depths {
            let depth = BigDecimal::from_str(dep_str).unwrap();
            let weight = calculate_volatility_weight(&variance_fixed, &depth);

            println!("深度単調性: depth={} → weight={}", depth, weight);
            assert!(weight >= prev_weight, "深度の増加で重みが減少しています");
            prev_weight = weight;
        }
    }

    #[test]
    fn test_volatility_weight_edge_cases() {
        use super::calculate_volatility_weight;

        println!("=== エッジケースのテスト ===");

        // 非常に小さな値
        let tiny_variance = BigDecimal::from_str("1e-10").unwrap();
        let tiny_depth = BigDecimal::from_str("1e-10").unwrap();
        let weight_tiny = calculate_volatility_weight(&tiny_variance, &tiny_depth);
        println!(
            "極小値: variance={}, depth={} → weight={}",
            tiny_variance, tiny_depth, weight_tiny
        );
        assert!(weight_tiny >= BigDecimal::from(0));

        // 非常に大きな値
        let huge_variance = BigDecimal::from_str("1e50").unwrap();
        let huge_depth = BigDecimal::from_str("1e50").unwrap();
        let weight_huge = calculate_volatility_weight(&huge_variance, &huge_depth);
        println!(
            "極大値: variance={}, depth={} → weight={}",
            huge_variance, huge_depth, weight_huge
        );
        assert!(weight_huge >= BigDecimal::from(0));
        assert!(weight_huge < BigDecimal::from_str("1e100").unwrap()); // 合理的な上限

        // f64範囲を超える値
        let beyond_f64_variance = BigDecimal::from_str("1e400").unwrap();
        let beyond_f64_depth = BigDecimal::from_str("1e400").unwrap();
        let weight_beyond = calculate_volatility_weight(&beyond_f64_variance, &beyond_f64_depth);
        println!(
            "f64超過: variance={}, depth={} → weight={}",
            beyond_f64_variance, beyond_f64_depth, weight_beyond
        );
        assert!(weight_beyond >= BigDecimal::from(0));
    }

    #[test]
    fn test_volatility_weight_financial_meaning() {
        use super::calculate_volatility_weight;

        println!("=== 金融的意味のテスト ===");

        // シナリオ1: 高ボラティリティ・低流動性（リスキー）
        let high_vol_low_liq = calculate_volatility_weight(
            &BigDecimal::from_str("100.0").unwrap(), // 高分散
            &BigDecimal::from_str("1.0").unwrap(),   // 低深度
        );

        // シナリオ2: 低ボラティリティ・高流動性（安全）
        let low_vol_high_liq = calculate_volatility_weight(
            &BigDecimal::from_str("1.0").unwrap(),   // 低分散
            &BigDecimal::from_str("100.0").unwrap(), // 高深度
        );

        // シナリオ3: 高ボラティリティ・高流動性（理想的）
        let high_vol_high_liq = calculate_volatility_weight(
            &BigDecimal::from_str("100.0").unwrap(), // 高分散
            &BigDecimal::from_str("100.0").unwrap(), // 高深度
        );

        println!("高ボラティリティ・低流動性: {}", high_vol_low_liq);
        println!("低ボラティリティ・高流動性: {}", low_vol_high_liq);
        println!("高ボラティリティ・高流動性: {}", high_vol_high_liq);

        // 期待される関係性：高ボラティリティ・高流動性が最高スコア
        assert!(high_vol_high_liq > high_vol_low_liq);
        assert!(high_vol_high_liq > low_vol_high_liq);

        // ボラティリティトレーダーにとって理想的なのは高ボラティリティ
        assert!(high_vol_low_liq > low_vol_high_liq);
    }

    // Integration tests would require database setup, so we'll focus on unit tests
    // The main sort_pools function is tested indirectly through the sort module tests
}
