//! 市場データ・分析・トークンメタデータモジュール
//!
//! トークンの価格履歴からボラティリティ・流動性スコアを計算し、
//! トークンメタデータの取得を提供する。

use crate::Result;
use bigdecimal::BigDecimal;
use common::algorithm::types::PriceHistory;
use common::types::*;
use num_traits::Zero;
use std::str::FromStr;

/// トークンメタデータ（NEP-148 準拠）
#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)] // デシリアライズ用に全フィールド必要
pub struct TokenMetadata {
    pub spec: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub reference: Option<String>,
    #[serde(default)]
    pub reference_hash: Option<String>,
}

/// トークンのメタデータを取得（ft_metadata）
pub async fn get_token_metadata<C>(client: &C, token_id: &str) -> Result<TokenMetadata>
where
    C: blockchain::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;

    let account_id: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({});
    let result = client
        .view_contract(&account_id, "ft_metadata", &args)
        .await?;

    // resultフィールドからJSONデータを取得してパース
    let metadata: TokenMetadata = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse token metadata: {}", e))?;

    Ok(metadata)
}

/// トークンの decimals を取得（キャッシュなし）
pub async fn get_token_decimals<C>(client: &C, token_id: &str) -> Result<u8>
where
    C: blockchain::jsonrpc::ViewContract,
{
    let metadata = get_token_metadata(client, token_id).await?;
    Ok(metadata.decimals)
}

/// 価格履歴からボラティリティを計算
pub fn calculate_volatility_from_history(history: &PriceHistory) -> Result<BigDecimal> {
    if history.prices.len() < 2 {
        return Err(anyhow::anyhow!(
            "Insufficient price data for volatility calculation: {} points",
            history.prices.len()
        ));
    }

    // 日次リターンを計算 (BigDecimalを使用)
    let returns: Vec<BigDecimal> = history
        .prices
        .windows(2)
        .filter_map(|window| {
            let prev_price = &window[0].price;
            let curr_price = &window[1].price;

            if prev_price.is_zero() {
                None
            } else {
                let return_rate = (curr_price - prev_price) / prev_price;
                Some(return_rate)
            }
        })
        .collect();

    if returns.is_empty() {
        return Err(anyhow::anyhow!(
            "No valid price returns for volatility calculation"
        ));
    }

    // 平均リターンを計算
    let sum: BigDecimal = returns.iter().sum();
    let count = BigDecimal::from(returns.len() as u64);
    let mean = &sum / &count;

    // 分散を計算
    let variance_sum: BigDecimal = returns
        .iter()
        .map(|r| {
            let diff = r - &mean;
            &diff * &diff
        })
        .sum();

    let variance = &variance_sum / &count;

    // BigDecimalで平方根を計算（Newton法による近似）
    if variance.is_zero() {
        return Ok(BigDecimal::from(0));
    }

    // 負の分散は無効
    if variance.sign() == bigdecimal::num_bigint::Sign::Minus {
        return Err(anyhow::anyhow!("Invalid negative variance"));
    }

    // Newton法による平方根計算
    let sqrt_variance = sqrt_bigdecimal(&variance)?;
    Ok(sqrt_variance)
}

/// 拡張された流動性スコアを計算（プール情報 + 取引量ベース）
/// 0.0 - 1.0 の範囲でスコアを返す
pub(crate) async fn calculate_enhanced_liquidity_score<C>(
    client: &C,
    token_id: &str,
    history: &PriceHistory,
) -> f64
where
    C: blockchain::jsonrpc::ViewContract,
{
    // 1. 基本的な取引量ベーススコア
    let volume_score = calculate_liquidity_score(history);

    // 2. REF Financeプール流動性スコア
    let pool_score = calculate_pool_liquidity_score(client, token_id).await;

    // 3. 両方のスコアを重み付き平均で統合
    let volume_weight = common::config::get("LIQUIDITY_VOLUME_WEIGHT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.6);
    let pool_weight = common::config::get("LIQUIDITY_POOL_WEIGHT")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.4);
    let combined_score = volume_score * volume_weight + pool_score * pool_weight;
    combined_score.clamp(0.0, 1.0)
}

/// プール流動性スコアを計算
async fn calculate_pool_liquidity_score<C>(client: &C, token_id: &str) -> f64
where
    C: blockchain::jsonrpc::ViewContract,
{
    use common::config;

    let ref_exchange_account = blockchain::ref_finance::CONTRACT_ADDRESS.clone();

    // config から設定取得（デフォルト 100 NEAR）
    let min_pool_liquidity_near: u32 = config::get("TRADE_MIN_POOL_LIQUIDITY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);

    let high_liquidity_threshold = (min_pool_liquidity_near as u128) * 10u128.pow(24);

    // プールで利用可能な流動性を取得
    match get_token_pool_liquidity(client, &ref_exchange_account, token_id).await {
        Ok(liquidity_amount) => {
            // 流動性をスコアに変換
            // score = 0.5 のとき liquidity == threshold
            let liquidity_ratio = liquidity_amount as f64 / high_liquidity_threshold as f64;

            // シグモイド的変換で 0.0-1.0 にマッピング
            let normalized_score = liquidity_ratio / (1.0 + liquidity_ratio);
            normalized_score.clamp(0.0, 1.0)
        }
        Err(_) => common::config::get("LIQUIDITY_ERROR_DEFAULT_SCORE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.3),
    }
}

/// トークンのプール流動性を取得
pub(crate) async fn get_token_pool_liquidity<C>(
    client: &C,
    ref_exchange_account: &near_sdk::AccountId,
    token_id: &str,
) -> Result<u128>
where
    C: blockchain::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;
    use serde_json::Value;

    // ft_balance_of でREF Exchangeでの残高を取得
    let token_account: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({
        "account_id": ref_exchange_account.to_string()
    });

    let result = client
        .view_contract(&token_account, "ft_balance_of", &args)
        .await?;

    let balance_json: Value = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse balance result: {}", e))?;

    if let Some(balance_str) = balance_json.as_str() {
        balance_str
            .parse::<u128>()
            .map_err(|e| anyhow::anyhow!("Failed to parse balance: {}", e))
    } else {
        Err(anyhow::anyhow!(
            "Expected string balance, got: {:?}",
            balance_json
        ))
    }
}

/// 基本的な流動性スコアを計算（取引量ベース）
/// 0.0 - 1.0 の範囲でスコアを返す
pub(crate) fn calculate_liquidity_score(history: &PriceHistory) -> f64 {
    // 取引量データがある価格ポイントを集計
    let volumes: Vec<&BigDecimal> = history
        .prices
        .iter()
        .filter_map(|p| p.volume.as_ref())
        .collect();

    if volumes.is_empty() {
        // 取引量データがない場合は中間値を返す
        return 0.5;
    }

    // 平均取引量を計算
    let sum: BigDecimal = volumes.iter().fold(BigDecimal::zero(), |acc, v| acc + *v);
    let count = BigDecimal::from(volumes.len() as u64);
    let avg_volume = &sum / &count;

    // 取引量を正規化（簡易版：10^24 yoctoNEAR を基準）
    let base_volume = BigDecimal::from(10u128.pow(24));
    let normalized = &avg_volume / &base_volume;

    // 0.0 - 1.0 の範囲に収める（シグモイド的な変換）
    let score = normalized
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);

    // 対数スケールで調整（大きな値を圧縮）
    if score > 0.0 {
        let ln_result = (score.ln() + 10.0) / 20.0;
        ln_result.clamp(0.0, 1.0) // 0-1の範囲に制限
    } else {
        0.1 // 最小値
    }
}

/// 市場規模を推定（実際の発行量データを取得）
///
/// # Arguments
/// * `client` - RPC クライアント
/// * `token_id` - トークン ID
/// * `price` - 価格（TokenPrice: NEAR/token）
/// * `decimals` - トークンの decimals
///
/// # Returns
/// * `NearValue` - 時価総額（NEAR 単位）
///
/// # 計算式
/// ```text
/// total_supply (TokenAmount) = get_token_total_supply(client, token_id, decimals)
/// market_cap (NearValue) = total_supply × price
/// ```
pub(crate) async fn estimate_market_cap_async<C>(
    client: &C,
    token_id: &str,
    price: &TokenPrice,
    decimals: u8,
) -> NearValue
where
    C: blockchain::jsonrpc::ViewContract,
{
    // 実際の発行量データを取得（TokenAmount）
    let total_supply = get_token_total_supply(client, token_id, decimals)
        .await
        .unwrap_or_else(|_| {
            // 取得失敗時は 10^24 smallest units と仮定
            TokenAmount::from_smallest_units(
                BigDecimal::from_str("1000000000000000000000000").unwrap(), // 10^24 smallest units
                decimals,
            )
        });

    if price.is_zero() {
        // デフォルト値: 10,000 NEAR
        return NearValue::from_near(BigDecimal::from(10000));
    }

    // market_cap (NearValue) = TokenAmount × TokenPrice
    &total_supply * price
}

/// トークンの総発行量を取得
///
/// # Arguments
/// * `client` - RPC クライアント
/// * `token_id` - トークン ID
/// * `decimals` - トークンの decimals
///
/// # Returns
/// * `TokenAmount` - 総発行量（smallest_units + decimals）
pub(crate) async fn get_token_total_supply<C>(
    client: &C,
    token_id: &str,
    decimals: u8,
) -> Result<TokenAmount>
where
    C: blockchain::jsonrpc::ViewContract,
{
    use near_sdk::AccountId;
    use serde_json::Value;

    let account_id: AccountId = token_id
        .parse()
        .map_err(|e| anyhow::anyhow!("Invalid token account ID: {}", e))?;

    let args = serde_json::json!({});
    let result = client
        .view_contract(&account_id, "ft_total_supply", &args)
        .await?;

    // resultフィールドからJSONデータを取得してパース
    let json_value: Value = serde_json::from_slice(&result.result)
        .map_err(|e| anyhow::anyhow!("Failed to parse result as JSON: {}", e))?;

    // total_supplyは通常文字列として返される
    if let Some(total_supply_str) = json_value.as_str() {
        let smallest_units = BigDecimal::from_str(total_supply_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse total supply: {}", e))?;
        Ok(TokenAmount::from_smallest_units(smallest_units, decimals))
    } else {
        Err(anyhow::anyhow!(
            "Expected string value for total supply, got: {:?}",
            json_value
        ))
    }
}

/// BigDecimalで平方根を計算（Newton法による近似）
pub(crate) fn sqrt_bigdecimal(value: &BigDecimal) -> Result<BigDecimal> {
    if value.is_zero() {
        return Ok(BigDecimal::from(0));
    }

    if value.sign() == bigdecimal::num_bigint::Sign::Minus {
        return Err(anyhow::anyhow!(
            "Cannot calculate square root of negative number"
        ));
    }

    // Newton法での近似計算
    let two = BigDecimal::from(2);
    // 精度を BigDecimal で直接設定 (1e-10 相当)
    let precision = BigDecimal::from(1) / BigDecimal::from(10000000000u64); // 1e-10

    // 初期推定値（入力値の半分）
    let mut x = value / &two;

    for _iteration in 0..50 {
        // 最大50回の反復
        let next_x = (&x + (value / &x)) / &two;

        // 収束判定
        let diff = if next_x > x {
            &next_x - &x
        } else {
            &x - &next_x
        };
        if diff < precision {
            return Ok(next_x);
        }

        x = next_x;
    }

    // 収束しなかった場合でも現在の近似値を返す
    Ok(x)
}

#[cfg(test)]
mod tests;
