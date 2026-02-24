use crate::Result;
use bigdecimal::BigDecimal;
use common::types::TokenAmount;
use logging::*;
use persistence::portfolio_holding::{NewPortfolioHolding, PortfolioHolding, TokenHolding};
use std::collections::BTreeMap;

/// トレード後のポートフォリオ保有量を DB に記録
pub async fn record_portfolio_holdings<C, W>(
    client: &C,
    wallet: &W,
    period_id: &str,
    selected_tokens: &[String],
) -> Result<()>
where
    C: blockchain::jsonrpc::AccountInfo
        + blockchain::jsonrpc::SendTx
        + blockchain::jsonrpc::ViewContract
        + blockchain::jsonrpc::GasInfo,
    W: blockchain::wallet::Wallet,
{
    let log = DEFAULT.new(o!("function" => "record_portfolio_holdings"));

    // wrap.near を含めて全残高を取得
    let wnear_str = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_string();
    let mut tokens: Vec<String> = selected_tokens.to_vec();
    if !tokens.contains(&wnear_str) {
        tokens.push(wnear_str);
    }

    let balances = crate::swap::get_current_portfolio_balances(client, wallet, &tokens).await?;

    // BTreeMap<String, TokenAmount> → Vec<TokenHolding> に変換
    let holdings = balances_to_holdings(&balances);

    if holdings.is_empty() {
        debug!(log, "no non-zero holdings to record");
        return Ok(());
    }

    let token_holdings = serde_json::to_value(&holdings)?;

    let record = NewPortfolioHolding {
        evaluation_period_id: period_id.to_string(),
        timestamp: chrono::Utc::now().naive_utc(),
        token_holdings,
    };

    PortfolioHolding::insert_async(record).await?;

    debug!(log, "recorded portfolio holdings";
        "period_id" => period_id,
        "holding_count" => holdings.len()
    );

    Ok(())
}

/// DB から最新の保有量を取得（RPC 置き換え用）
///
/// レコードなしの場合は `None` を返す（呼び出し側で RPC にフォールバック）
pub async fn get_holdings_from_db(
    period_id: &str,
) -> Result<Option<BTreeMap<String, TokenAmount>>> {
    let record = PortfolioHolding::get_latest_for_period_async(period_id.to_string()).await?;

    let record = match record {
        Some(r) => r,
        None => return Ok(None),
    };

    let holdings = record.parse_holdings()?;
    holdings_to_balances(&holdings).map(Some)
}

/// 古い保有量レコードのクリーンアップ
pub async fn cleanup_old_records() -> Result<usize> {
    let retention_days: i64 = common::config::get("PORTFOLIO_HOLDINGS_RETENTION_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(90);

    PortfolioHolding::cleanup_old_records(retention_days).await
}

/// BTreeMap<String, TokenAmount> → Vec<TokenHolding> に変換（ゼロ残高は除外）
fn balances_to_holdings(balances: &BTreeMap<String, TokenAmount>) -> Vec<TokenHolding> {
    balances
        .iter()
        .filter(|(_, amount)| !amount.is_zero())
        .map(|(token, amount)| TokenHolding {
            token: token.clone(),
            balance: amount.smallest_units().to_string(),
            decimals: amount.decimals(),
        })
        .collect()
}

/// Vec<TokenHolding> → BTreeMap<String, TokenAmount> に変換
fn holdings_to_balances(holdings: &[TokenHolding]) -> Result<BTreeMap<String, TokenAmount>> {
    let mut result = BTreeMap::new();
    for h in holdings {
        let smallest_units: BigDecimal = h
            .balance
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse balance '{}': {}", h.balance, e))?;
        result.insert(
            h.token.clone(),
            TokenAmount::from_smallest_units(smallest_units, h.decimals),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
