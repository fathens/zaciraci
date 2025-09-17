mod arima;

use crate::Result;
use crate::config;
use crate::logging::*;
use crate::persistence::TimeRange;
use crate::persistence::token_rate::TokenRate;
use crate::ref_finance::token_account::{TokenInAccount, TokenOutAccount};
use crate::trade::predict::PredictionService;
use crate::types::MilliNear;
use bigdecimal::BigDecimal;
use chrono::{Duration, NaiveDateTime, Utc};
use futures_util::future::join_all;
use near_sdk::AccountId;
use num_traits::Zero;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use zaciraci_common::algorithm::{
    TokenHolding,
    portfolio::{PortfolioData, execute_portfolio_optimization},
    types::{PriceHistory, TokenData, TradingAction, WalletInfo},
};

#[derive(Clone)]
pub struct SameBaseTokenRates {
    #[allow(dead_code)]
    pub base: TokenOutAccount,
    #[allow(dead_code)]
    pub quote: TokenInAccount,
    pub points: Vec<Point>,
}

#[derive(Clone)]
pub struct Point {
    pub rate: BigDecimal,
    pub timestamp: NaiveDateTime,
}

pub struct StatsInPeriod<U> {
    pub timestamp: NaiveDateTime,
    pub period: Duration,

    pub start: U,
    pub end: U,
    pub average: U,
    pub max: U,
    pub min: U,
}
pub struct ListStatsInPeriod<U>(Vec<StatsInPeriod<U>>);

pub async fn start() -> Result<()> {
    let log = DEFAULT.new(o!("function" => "trade::start"));

    info!(log, "starting portfolio-based trading strategy");

    // Step 1: 資金準備 (NEAR -> wrap.near)
    let available_funds = prepare_funds().await?;
    info!(log, "Prepared funds"; "available_funds" => available_funds);

    if available_funds == 0 {
        info!(log, "no funds available for trading");
        return Ok(());
    }

    // Step 2: トークン選定 (top volatility)
    let selected_tokens = select_top_volatility_tokens().await?;
    info!(log, "Selected tokens"; "count" => selected_tokens.len());

    if selected_tokens.is_empty() {
        info!(log, "no tokens selected for trading");
        return Ok(());
    }

    // Step 3: PredictionServiceの初期化
    let chronos_url =
        std::env::var("CHRONOS_URL").unwrap_or_else(|_| "http://localhost:8000".to_string());
    let backend_url =
        std::env::var("BACKEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

    let prediction_service = PredictionService::new(chronos_url, backend_url);

    // Step 4: ポートフォリオ最適化と実行
    match execute_portfolio_strategy(&prediction_service, &selected_tokens, available_funds).await {
        Ok(report) => {
            info!(log, "portfolio strategy executed successfully";
                "total_actions" => report.len()
            );

            // 実際の取引実行は将来の実装で追加
            for action in report {
                info!(log, "trading action"; "action" => ?action);
            }

            // Step 5: ハーベスト判定と実行
            check_and_harvest(available_funds).await?;
        }
        Err(e) => {
            error!(log, "failed to execute portfolio strategy"; "error" => ?e);
        }
    }

    info!(log, "success");
    Ok(())
}

/// 現在の保有トークンを取得（仮の実装）
#[allow(dead_code)]
async fn get_current_holdings() -> Result<Vec<TokenHolding>> {
    let log = DEFAULT.new(o!("function" => "get_current_holdings"));

    // 実際の実装では、ウォレットAPIまたはDBから保有情報を取得
    // ここでは仮のデータを返す
    let holdings = vec![TokenHolding {
        token: "wrap.near".to_string(),
        amount: BigDecimal::from(100),
        current_price: BigDecimal::from(1), // 1 NEAR = 1として仮定
    }];

    info!(log, "retrieved holdings"; "count" => holdings.len());
    Ok(holdings)
}

/// 資金準備 (NEAR -> wrap.near 変換)
async fn prepare_funds() -> Result<u128> {
    let log = DEFAULT.new(o!("function" => "prepare_funds"));

    // TODO: 実際のNEAR残高の取得とwrap.near変換を実装
    // 現在は仮の値を返す
    let available_funds = config::get("TRADE_INITIAL_INVESTMENT")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(|v| MilliNear::of(v as u32).to_yocto())
        .unwrap_or_else(|| MilliNear::of(100).to_yocto());

    info!(log, "prepared funds"; "amount" => available_funds);
    Ok(available_funds)
}

/// トップボラティリティトークンの選定
async fn select_top_volatility_tokens() -> Result<Vec<AccountId>> {
    let log = DEFAULT.new(o!("function" => "select_top_volatility_tokens"));

    let limit = config::get("TRADE_TOP_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    // 30日間のデータから高ボラティリティトークンを選択
    let end_time = Utc::now().naive_utc();
    let start_time = end_time - Duration::days(30);
    let _time_range = TimeRange {
        start: start_time,
        end: end_time,
    };

    // TODO: 実際のボラティリティ計算とトークン選定を実装
    // 現在は仮のトークンリストを返す
    let tokens: Vec<AccountId> = vec![
        "akaia.tkn.near".parse::<AccountId>()?,
        "babyblackdragon.tkn.near".parse::<AccountId>()?,
        "nearkat.tkn.near".parse::<AccountId>()?,
    ]
    .into_iter()
    .take(limit)
    .collect();

    info!(log, "selected tokens"; "count" => tokens.len());
    Ok(tokens)
}

/// ポートフォリオ戦略の実行
async fn execute_portfolio_strategy(
    _prediction_service: &PredictionService,
    tokens: &[AccountId],
    available_funds: u128,
) -> Result<Vec<TradingAction>> {
    let log = DEFAULT.new(o!("function" => "execute_portfolio_strategy"));

    // ポートフォリオデータの準備
    let mut token_data = Vec::new();
    let mut predictions = BTreeMap::new();
    let mut historical_prices = Vec::new();

    for token in tokens {
        // 現在価格の取得 (仮実装)
        let current_price = get_token_current_price(token).await?;

        // 予測の取得 (仮実装)
        // TODO: PredictionServiceのget_predictionメソッドを実装
        let prediction = get_prediction_placeholder(token).await?;
        predictions.insert(token.to_string(), prediction);

        // 履歴価格の取得
        let history = get_token_price_history(token).await?;
        historical_prices.push(history);

        token_data.push(TokenData {
            symbol: token.to_string(),
            current_price: BigDecimal::from(current_price),
            historical_volatility: 0.2, // 仮の値
            liquidity_score: Some(0.5), // 仮の値
            market_cap: Some(10000.0),  // 仮の値
            decimals: Some(24),
        });
    }

    let portfolio_data = PortfolioData {
        tokens: token_data,
        predictions,
        historical_prices,
        correlation_matrix: None,
    };

    let wallet_info = WalletInfo {
        holdings: BTreeMap::new(),
        total_value: available_funds as f64,
        cash_balance: available_funds as f64,
    };

    // ポートフォリオ最適化の実行
    let execution_report = execute_portfolio_optimization(
        &wallet_info,
        portfolio_data,
        0.1, // rebalance threshold
    )
    .await?;

    info!(log, "portfolio optimization completed";
        "actions" => execution_report.actions.len()
    );

    Ok(execution_report.actions)
}

/// ハーベスト判定と実行
async fn check_and_harvest(initial_amount: u128) -> Result<()> {
    let log = DEFAULT.new(o!("function" => "check_and_harvest"));

    // TODO: 実際のポートフォリオ価値計算とハーベスト実行を実装
    // 現在の値: 仮の値として初期投資額の150%とする
    let current_value = (initial_amount as f64 * 1.5) as u128;
    let threshold = (initial_amount as f64 * 2.0) as u128; // 200%

    if current_value > threshold {
        let excess = current_value - threshold;
        let harvest_amount = (excess as f64 * 0.1) as u128; // 10%

        let min_harvest = config::get("HARVEST_MIN_AMOUNT")
            .ok()
            .and_then(|v| v.parse::<u128>().ok())
            .unwrap_or(10_000_000_000_000_000_000_000_000); // 10 NEAR

        if harvest_amount >= min_harvest {
            // TODO: 実際のハーベスト処理
            info!(log, "harvest executed"; "amount" => harvest_amount);
        } else {
            info!(log, "harvest amount below minimum";
                "amount" => harvest_amount,
                "minimum" => min_harvest
            );
        }
    } else {
        info!(log, "harvest threshold not reached";
            "current" => current_value,
            "threshold" => threshold
        );
    }

    Ok(())
}

/// トークンの現在価格を取得 (仮実装)
async fn get_token_current_price(token: &AccountId) -> Result<u128> {
    let log = DEFAULT.new(o!("function" => "get_token_current_price"));

    // TODO: 実際の価格取得APIを実装
    let price = 1_000_000_000_000_000_000_000_000; // 1 NEAR相当として仮設定

    info!(log, "retrieved token price"; "token" => %token, "price" => price);
    Ok(price)
}

/// トークンの価格履歴を取得 (仮実装)
async fn get_token_price_history(token: &AccountId) -> Result<PriceHistory> {
    let log = DEFAULT.new(o!("function" => "get_token_price_history"));

    // TODO: 実際の価格履歴取得を実装
    let history = PriceHistory {
        token: token.to_string(),
        quote_token: "wrap.near".to_string(),
        prices: vec![], // 空のデータ
    };

    info!(log, "retrieved price history"; "token" => %token);
    Ok(history)
}

/// 予測値を取得するためのプレースホルダー関数
async fn get_prediction_placeholder(_token: &AccountId) -> Result<f64> {
    // TODO: 実際のPredictionServiceのget_predictionメソッドを実装
    Ok(0.1) // 仮の予測値（10%のリターン）
}

#[allow(dead_code)]
async fn forcast_rates(
    range: &TimeRange,
    period: Duration,
    target: NaiveDateTime,
) -> Result<HashMap<TokenOutAccount, BigDecimal>> {
    let log = DEFAULT.new(o!("function" => "trade::forcast_rates"));
    info!(log, "start");
    let quote = get_top_quote_token(range).await?;
    let bases = get_base_tokens(range, &quote).await?;
    let ps = bases.iter().map(|base| async {
        let rates = SameBaseTokenRates::load(&quote, base, range).await?;
        let result = rates.forcast(period, target).await?;
        Ok((base.clone(), result))
    });
    let rates_by_base = join_all(ps).await;
    info!(log, "success");
    rates_by_base.into_iter().collect()
}

#[allow(dead_code)]
async fn get_top_quote_token(range: &TimeRange) -> Result<TokenInAccount> {
    let log = DEFAULT.new(o!("function" => "trade::get_top_quote_token"));

    let quotes = TokenRate::get_quotes_in_time_range(range).await?;
    let (quote, _) = quotes.iter().max_by_key(|(_, c)| *c).unwrap();

    info!(log, "success");
    Ok(quote.clone())
}

#[allow(dead_code)]
async fn get_base_tokens(
    range: &TimeRange,
    quote: &TokenInAccount,
) -> Result<Vec<TokenOutAccount>> {
    let log = DEFAULT.new(o!("function" => "trade::get_base_tokens"));

    let bases = TokenRate::get_bases_in_time_range(range, quote).await?;
    let max_count = bases.iter().max_by_key(|(_, c)| *c).unwrap().1;
    let limit = max_count / 2;
    let tokens = bases
        .iter()
        .filter(|(_, c)| *c > limit)
        .map(|(t, _)| t.clone())
        .collect();

    info!(log, "success");
    Ok(tokens)
}

impl SameBaseTokenRates {
    pub async fn load(
        quote: &TokenInAccount,
        base: &TokenOutAccount,
        range: &TimeRange,
    ) -> Result<Self> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::load",
            "base" => base.to_string(),
            "quote" => quote.to_string(),
            "start" => format!("{:?}", range.start),
            "end" => format!("{:?}", range.end),
        ));
        info!(log, "start");
        match TokenRate::get_rates_in_time_range(range, base, quote).await {
            Ok(rates) => {
                info!(log, "loaded rates"; "rates_count" => rates.len());
                let points = rates
                    .iter()
                    .map(|r| Point {
                        rate: r.rate.clone(),
                        timestamp: r.timestamp,
                    })
                    .collect();
                Ok(SameBaseTokenRates {
                    base: base.clone(),
                    quote: quote.clone(),
                    points,
                })
            }
            Err(e) => {
                error!(log, "Failed to get rates"; "error" => ?e);
                Err(e)
            }
        }
    }

    #[allow(dead_code)]
    async fn forcast(&self, period: Duration, target: NaiveDateTime) -> Result<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::forcast",
            "period" => format!("{}", period),
            "target" => format!("{:?}", target),
        ));
        info!(log, "start");

        let stats = self.aggregate(period);
        let _descs = stats.describes();

        // arima モジュールの予測関数を使用して将来の値を予測
        let result = arima::predict_future_rate(&self.points, target)?;

        info!(log, "success"; "predicted_rate" => %result);
        Ok(result)
    }

    pub fn aggregate(&self, period: Duration) -> ListStatsInPeriod<BigDecimal> {
        let log = DEFAULT.new(o!(
            "function" => "SameBaseTokenRates::aggregate",
            "rates_count" => self.points.len(),
            "period" => format!("{}", period),
        ));
        info!(log, "start");

        if self.points.is_empty() {
            return ListStatsInPeriod(Vec::new());
        }

        // タイムスタンプの最小値と最大値を取得
        let min_time = self.points.first().unwrap().timestamp;
        let max_time = self.points.last().unwrap().timestamp;

        // 期間ごとに統計を計算
        let mut stats = Vec::new();
        let mut current_start = min_time;

        while current_start <= max_time {
            let current_end = current_start + period;
            let rates_in_period: Vec<_> = self
                .points
                .iter()
                .skip_while(|rate| rate.timestamp < current_start)
                .take_while(|rate| rate.timestamp < current_end)
                .collect();

            if !rates_in_period.is_empty() {
                let start = rates_in_period.first().unwrap().rate.clone();
                let end = rates_in_period.last().unwrap().rate.clone();
                let values: Vec<_> = rates_in_period.iter().map(|tr| tr.rate.clone()).collect();
                let sum: BigDecimal = values.iter().sum();
                let count = BigDecimal::from(values.len() as i64);
                let average = &sum / &count;
                let max = values.iter().max().unwrap().clone();
                let min = values.iter().min().unwrap().clone();

                stats.push(StatsInPeriod {
                    timestamp: current_start,
                    period,
                    start,
                    end,
                    average,
                    max,
                    min,
                });
            }

            current_start = current_end;
        }

        info!(log, "success"; "stats_count" => stats.len());
        ListStatsInPeriod(stats)
    }
}

impl<U> ListStatsInPeriod<U>
where
    U: Clone + Display,
    U: Add<Output = U> + Sub<Output = U> + Mul<Output = U> + Div<Output = U>,
    U: Zero + PartialOrd + From<i64>,
{
    fn format_decimal(value: U) -> String {
        let s = value.to_string();
        if s.contains('.') {
            // 小数点以下の末尾の0を削除し、最大9桁まで表示
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() == 2 {
                let integer_part = parts[0];
                let mut decimal_part = parts[1];

                // 小数点以下が全て0の場合は整数表示
                if decimal_part.chars().all(|c| c == '0') {
                    return integer_part.to_string();
                }

                // 末尾の0を削除
                decimal_part = decimal_part.trim_end_matches('0');

                // 小数点以下が9桁を超える場合は9桁までに制限
                if decimal_part.len() > 9 {
                    decimal_part = &decimal_part[..9];
                }

                // 小数点以下が空になった場合は整数のみ返す
                if decimal_part.is_empty() {
                    return integer_part.to_string();
                }

                format!("{}.{}", integer_part, decimal_part)
            } else {
                s
            }
        } else {
            s
        }
    }

    pub fn describes(&self) -> Vec<String> {
        let log = DEFAULT.new(o!(
            "function" => "ListStatsInPeriod::describes",
            "stats_count" => self.0.len(),
        ));
        info!(log, "start");
        let mut lines = Vec::new();
        let mut prev = None;
        for stat in self.0.iter() {
            let date = stat.timestamp.to_string();
            let changes = prev
                .map(|p: &StatsInPeriod<U>| {
                    let prev = format!(
                        "from the previous {m} minutes",
                        m = stat.period.num_minutes()
                    );
                    let diff = stat.end.clone() - p.end.clone();
                    if diff.is_zero() {
                        return format!(", no change {prev}");
                    }
                    let dw = if diff < U::zero() {
                        "decrease"
                    } else {
                        "increase"
                    };
                    let change = (diff / p.end.clone()) * 100_i64.into();
                    let change_str = Self::format_decimal(change);
                    format!(", marking a {change_str} % {dw} {prev}")
                })
                .unwrap_or_default();
            let summary = format!(
                "opened at {start}, closed at {end}, with a high of {max}, a low of {min}, and an average of {ave}",
                start = Self::format_decimal(stat.start.clone()),
                end = Self::format_decimal(stat.end.clone()),
                max = Self::format_decimal(stat.max.clone()),
                min = Self::format_decimal(stat.min.clone()),
                ave = Self::format_decimal(stat.average.clone()),
            );
            let line = format!("{date}, {summary}{changes}");
            debug!(log, "added line";
                "line" => &line,
            );
            lines.push(line);
            prev = Some(stat);
        }
        info!(log, "success";
           "lines_count" => lines.len(),
        );
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ref_finance::token_account::TokenAccount;
    use std::str::FromStr;

    #[test]
    fn test_describes() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![]);
        assert!(stats.describes().is_empty());
    }

    #[test]
    fn test_describes_increase() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(101),
                end: BigDecimal::from(100),
                max: BigDecimal::from(102),
                min: BigDecimal::from(90),
                average: BigDecimal::from(95),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(150),
                max: BigDecimal::from(155),
                min: BigDecimal::from(140),
                average: BigDecimal::from(147),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("increase"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 101, closed at 100, with a high of 102, a low of 90, and an average of 95",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 150, with a high of 155, a low of 140, and an average of 147, marking a 50 % increase from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_decrease() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(100),
                max: BigDecimal::from(100),
                min: BigDecimal::from(100),
                average: BigDecimal::from(100),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from(100),
                end: BigDecimal::from(50),
                max: BigDecimal::from(50),
                min: BigDecimal::from(50),
                average: BigDecimal::from(50),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("decrease"));
        assert!(descriptions[1].contains("50 %"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100, closed at 100, with a high of 100, a low of 100, and an average of 100",
                "2025-03-27 11:37:48.196150, opened at 100, closed at 50, with a high of 50, a low of 50, and an average of 50, marking a -50 % decrease from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_describes_no_change() {
        let stats: ListStatsInPeriod<BigDecimal> = ListStatsInPeriod(vec![
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-26 11:37:48.195977",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from_str("100.123456789").unwrap(),
                end: BigDecimal::from_str("100.123456789").unwrap(),
                max: BigDecimal::from_str("100.123456789").unwrap(),
                min: BigDecimal::from_str("100.123456789").unwrap(),
                average: BigDecimal::from_str("100.123456789").unwrap(),
            },
            StatsInPeriod {
                timestamp: NaiveDateTime::parse_from_str(
                    "2025-03-27 11:37:48.196150",
                    "%Y-%m-%d %H:%M:%S%.f",
                )
                .unwrap(),
                period: Duration::minutes(1),
                start: BigDecimal::from_str("100.123456789").unwrap(),
                end: BigDecimal::from_str("100.123456789").unwrap(),
                max: BigDecimal::from_str("100.123456789").unwrap(),
                min: BigDecimal::from_str("100.123456789").unwrap(),
                average: BigDecimal::from_str("100.123456789").unwrap(),
            },
        ]);
        let descriptions = stats.describes();
        assert_eq!(descriptions.len(), 2);
        assert!(descriptions[1].contains("no change"));
        assert_eq!(
            descriptions,
            vec![
                "2025-03-26 11:37:48.195977, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789",
                "2025-03-27 11:37:48.196150, opened at 100.123456789, closed at 100.123456789, with a high of 100.123456789, a low of 100.123456789, and an average of 100.123456789, no change from the previous 1 minutes"
            ]
        );
    }

    #[test]
    fn test_stats_empty() {
        // 空のポイントリストを持つSameBaseTokenRatesを作成
        let rates = SameBaseTokenRates {
            points: Vec::new(),
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果が空のベクターであることを確認
        assert!(stats.0.is_empty());
    }

    #[test]
    fn test_stats_single_period() {
        // 1つの期間内に複数のポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            Point {
                timestamp: base_time + Duration::seconds(20),
                rate: BigDecimal::from(110),
            },
            Point {
                timestamp: base_time + Duration::seconds(40),
                rate: BigDecimal::from(90),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果を検証
        assert_eq!(stats.0.len(), 1);
        let stat = &stats.0[0];

        assert_eq!(stat.timestamp, base_time);
        assert_eq!(stat.period, Duration::minutes(1));
        assert_eq!(stat.start, BigDecimal::from(100));
        assert_eq!(stat.end, BigDecimal::from(90));
        assert_eq!(stat.max, BigDecimal::from(110));
        assert_eq!(stat.min, BigDecimal::from(90));

        // 平均値の検証 (100 + 110 + 90) / 3 = 100
        assert_eq!(stat.average, BigDecimal::from(100));
    }

    #[test]
    fn test_stats_multiple_periods() {
        // 複数の期間にまたがるポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            // 最初の期間 (10:00:00 - 10:01:00)
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            Point {
                timestamp: base_time + Duration::seconds(30),
                rate: BigDecimal::from(110),
            },
            // 2番目の期間 (10:01:00 - 10:02:00)
            Point {
                timestamp: base_time + Duration::minutes(1),
                rate: BigDecimal::from(120),
            },
            Point {
                timestamp: base_time + Duration::minutes(1) + Duration::seconds(30),
                rate: BigDecimal::from(130),
            },
            // 3番目の期間 (10:02:00 - 10:03:00)
            Point {
                timestamp: base_time + Duration::minutes(2),
                rate: BigDecimal::from(140),
            },
            Point {
                timestamp: base_time + Duration::minutes(2) + Duration::seconds(30),
                rate: BigDecimal::from(150),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 1分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(1));

        // 結果を検証
        assert_eq!(stats.0.len(), 3);

        // 最初の期間の検証
        {
            let stat = &stats.0[0];
            assert_eq!(stat.timestamp, base_time);
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(100));
            assert_eq!(stat.end, BigDecimal::from(110));
            assert_eq!(stat.max, BigDecimal::from(110));
            assert_eq!(stat.min, BigDecimal::from(100));
            assert_eq!(stat.average, BigDecimal::from(105)); // (100 + 110) / 2 = 105
        }

        // 2番目の期間の検証
        {
            let stat = &stats.0[1];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(1));
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(120));
            assert_eq!(stat.end, BigDecimal::from(130));
            assert_eq!(stat.max, BigDecimal::from(130));
            assert_eq!(stat.min, BigDecimal::from(120));
            assert_eq!(stat.average, BigDecimal::from(125)); // (120 + 130) / 2 = 125
        }

        // 3番目の期間の検証
        {
            let stat = &stats.0[2];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(2));
            assert_eq!(stat.period, Duration::minutes(1));
            assert_eq!(stat.start, BigDecimal::from(140));
            assert_eq!(stat.end, BigDecimal::from(150));
            assert_eq!(stat.max, BigDecimal::from(150));
            assert_eq!(stat.min, BigDecimal::from(140));
            assert_eq!(stat.average, BigDecimal::from(145)); // (140 + 150) / 2 = 145
        }
    }

    #[test]
    fn test_stats_period_boundary() {
        // 期間の境界値をテストするためのポイントを持つSameBaseTokenRatesを作成
        let base_time =
            NaiveDateTime::parse_from_str("2025-03-26 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let points = vec![
            // 最初の期間 (10:00:00 - 10:05:00)
            Point {
                timestamp: base_time,
                rate: BigDecimal::from(100),
            },
            // 境界値ちょうど (10:05:00) - 次の期間に含まれる
            Point {
                timestamp: base_time + Duration::minutes(5),
                rate: BigDecimal::from(200),
            },
            // 2番目の期間 (10:05:00 - 10:10:00)
            Point {
                timestamp: base_time + Duration::minutes(7),
                rate: BigDecimal::from(300),
            },
        ];

        let rates = SameBaseTokenRates {
            points,
            base: "wrap.near".parse::<TokenAccount>().unwrap().into(),
            quote: "usdt.tether-token.near"
                .parse::<TokenAccount>()
                .unwrap()
                .into(),
        };

        // 5分間の期間で統計を計算
        let stats = rates.aggregate(Duration::minutes(5));

        // 結果を検証
        assert_eq!(stats.0.len(), 2);

        // 最初の期間の検証
        {
            let stat = &stats.0[0];
            assert_eq!(stat.timestamp, base_time);
            assert_eq!(stat.period, Duration::minutes(5));
            assert_eq!(stat.start, BigDecimal::from(100));
            assert_eq!(stat.end, BigDecimal::from(100));
            assert_eq!(stat.max, BigDecimal::from(100));
            assert_eq!(stat.min, BigDecimal::from(100));
            assert_eq!(stat.average, BigDecimal::from(100));
        }

        // 2番目の期間の検証 (境界値を含む)
        {
            let stat = &stats.0[1];
            assert_eq!(stat.timestamp, base_time + Duration::minutes(5));
            assert_eq!(stat.period, Duration::minutes(5));
            assert_eq!(stat.start, BigDecimal::from(200));
            assert_eq!(stat.end, BigDecimal::from(300));
            assert_eq!(stat.max, BigDecimal::from(300));
            assert_eq!(stat.min, BigDecimal::from(200));
            assert_eq!(stat.average, BigDecimal::from(250)); // (200 + 300) / 2 = 250
        }
    }

    #[test]
    fn test_format_decimal_digits() {
        // 整数値のテスト
        assert_eq!(
            "100",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from(100))
        );

        // 小数点以下が全て0の値
        let with_zeros = BigDecimal::from(100) + BigDecimal::from_str("0.000000000").unwrap();
        assert_eq!(
            "100",
            ListStatsInPeriod::<BigDecimal>::format_decimal(with_zeros)
        );

        // 小数点以下が1桁の値
        assert_eq!(
            "0.1",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.1").unwrap())
        );

        // 小数点以下が2桁の値
        assert_eq!(
            "0.12",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.12").unwrap())
        );

        // 小数点以下が3桁の値
        assert_eq!(
            "0.123",
            ListStatsInPeriod::<BigDecimal>::format_decimal(BigDecimal::from_str("0.123").unwrap())
        );

        // 小数点以下が4桁の値
        assert_eq!(
            "0.1234",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234").unwrap()
            )
        );

        // 小数点以下が5桁の値
        assert_eq!(
            "0.12345",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345").unwrap()
            )
        );

        // 小数点以下が6桁の値
        assert_eq!(
            "0.123456",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.123456").unwrap()
            )
        );

        // 小数点以下が7桁の値
        assert_eq!(
            "0.1234567",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234567").unwrap()
            )
        );

        // 小数点以下が8桁の値
        assert_eq!(
            "0.12345678",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345678").unwrap()
            )
        );

        // 小数点以下が9桁の値
        assert_eq!(
            "0.123456789",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.123456789").unwrap()
            )
        );

        // 小数点以下が10桁の値（9桁までに制限される）
        assert_eq!(
            "0.123456789",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.1234567891").unwrap()
            )
        );

        // 末尾に0がある場合（末尾の0は削除される）
        assert_eq!(
            "0.12345",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("0.12345000").unwrap()
            )
        );

        // 整数部分あり、小数点以下4桁の値
        assert_eq!(
            "123.4567",
            ListStatsInPeriod::<BigDecimal>::format_decimal(
                BigDecimal::from_str("123.4567").unwrap()
            )
        );
    }
}
