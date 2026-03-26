use crate::Result;
use common::types::{ExchangeRate, NearValue, TokenAccount, TokenAmount, TokenOutAccount};
use logging::*;
use std::collections::BTreeMap;

/// レート取得の抽象化。実装ごとにデータソースが異なる。
pub trait RateProvider: Send + Sync {
    fn get_rate(
        &self,
        token: &TokenOutAccount,
    ) -> impl std::future::Future<Output = Result<Option<ExchangeRate>>> + Send;
}

/// 最新の DB レートを返す RateProvider 実装
pub struct LatestRateProvider;

impl RateProvider for LatestRateProvider {
    async fn get_rate(&self, token: &TokenOutAccount) -> Result<Option<ExchangeRate>> {
        use persistence::token_rate::TokenRate;

        let quote_token = blockchain::ref_finance::token_account::WNEAR_TOKEN.to_in();
        let rate = TokenRate::get_latest(token, &quote_token).await?;
        Ok(rate.map(|r| r.to_spot_rate()))
    }
}

/// ポートフォリオ総価値を計算（NEAR 単位）
pub async fn calculate_portfolio_value(
    holdings: &BTreeMap<TokenAccount, TokenAmount>,
    rate_provider: &impl RateProvider,
) -> Result<NearValue> {
    let log = DEFAULT.new(o!("function" => "calculate_portfolio_value"));
    let mut total_value = NearValue::zero();

    let wnear_token = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;

    for (token, amount) in holdings {
        if amount.is_zero() {
            continue;
        }

        if token == wnear_token {
            let rate = ExchangeRate::wnear();
            let value = amount / &rate;
            total_value = total_value + value;
        } else {
            let base_token: TokenOutAccount = token.clone().into();
            match rate_provider.get_rate(&base_token).await {
                Ok(Some(spot)) => {
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

#[cfg(test)]
mod tests;
