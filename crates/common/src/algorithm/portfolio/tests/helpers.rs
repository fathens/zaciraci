use super::*;

pub fn token_out(s: &str) -> TokenOutAccount {
    s.parse().unwrap()
}

pub fn token_in(s: &str) -> TokenInAccount {
    s.parse().unwrap()
}

// ==================== テストヘルパー ====================

pub fn price(v: f64) -> TokenPrice {
    TokenPrice::from_near_per_token(BigDecimal::from_f64(v).unwrap())
}

/// ExchangeRate を price (NEAR/token) から作成するヘルパー
///
/// 使用例:
/// - rate_from_price(0.01) → 0.01 NEAR/token = 100 tokens/NEAR
pub fn rate_from_price(near_per_token: f64) -> ExchangeRate {
    ExchangeRate::from_price(&price(near_per_token), 18)
}

pub fn cap(v: i64) -> NearValue {
    NearValue::from_near(BigDecimal::from(v))
}

pub fn create_sample_tokens() -> Vec<TokenInfo> {
    vec![
        TokenInfo {
            symbol: token_out("token-a"),
            current_rate: rate_from_price(0.01),
            historical_volatility: 0.2,
            liquidity_score: Some(0.8),
            market_cap: Some(cap(1000000)),
        },
        TokenInfo {
            symbol: token_out("token-b"),
            current_rate: rate_from_price(0.02),
            historical_volatility: 0.3,
            liquidity_score: Some(0.7),
            market_cap: Some(cap(500000)),
        },
        TokenInfo {
            symbol: token_out("token-c"),
            current_rate: rate_from_price(0.005),
            historical_volatility: 0.1,
            liquidity_score: Some(0.9),
            market_cap: Some(cap(2000000)),
        },
    ]
}

pub fn create_sample_predictions() -> BTreeMap<TokenOutAccount, TokenPrice> {
    // predictions は予測価格（TokenPrice: NEAR/token）を表す
    // 価格上昇 = 正のリターン
    // current_rate = rate_from_price(0.01) → 0.01 NEAR/token
    // +10% リターン: predicted_price = current_price * 1.1
    let mut predictions = BTreeMap::new();
    predictions.insert(token_out("token-a"), price(0.01 * 1.1)); // current=0.01, +10%
    predictions.insert(token_out("token-b"), price(0.02 * 1.1)); // current=0.02, +10%
    predictions.insert(token_out("token-c"), price(0.005 * 1.05)); // current=0.005, +5%
    predictions
}

pub fn create_sample_price_history() -> BTreeMap<TokenOutAccount, PriceHistory> {
    let base_time = Utc::now() - TimeDelta::days(30);
    let mut history = BTreeMap::new();

    // TOKEN_A: 上昇トレンド
    let mut token_a_prices = Vec::new();
    for i in 0..30 {
        token_a_prices.push(PricePoint {
            timestamp: base_time + TimeDelta::days(i),
            price: price(90.0 + i as f64 * 0.5),
            volume: Some(BigDecimal::from_f64(1000.0).unwrap()),
        });
    }
    let ph = PriceHistory {
        token: token_out("token-a"),
        quote_token: token_in("wrap.near"),
        prices: token_a_prices,
    };
    history.insert(ph.token.clone(), ph);

    // TOKEN_B: 変動大
    let mut token_b_prices = Vec::new();
    for i in 0..30 {
        let volatility = ((i as f64 * 0.2).sin() * 10.0) + 50.0;
        token_b_prices.push(PricePoint {
            timestamp: base_time + TimeDelta::days(i),
            price: price(volatility),
            volume: Some(BigDecimal::from_f64(800.0).unwrap()),
        });
    }
    let ph = PriceHistory {
        token: token_out("token-b"),
        quote_token: token_in("wrap.near"),
        prices: token_b_prices,
    };
    history.insert(ph.token.clone(), ph);

    // TOKEN_C: 安定
    let mut token_c_prices = Vec::new();
    for i in 0..30 {
        token_c_prices.push(PricePoint {
            timestamp: base_time + TimeDelta::days(i),
            price: price(195.0 + (i as f64 * 0.2)),
            volume: Some(BigDecimal::from_f64(1200.0).unwrap()),
        });
    }
    let ph = PriceHistory {
        token: token_out("token-c"),
        quote_token: token_in("wrap.near"),
        prices: token_c_prices,
    };
    history.insert(ph.token.clone(), ph);

    history
}

pub fn create_sample_wallet() -> WalletInfo {
    let mut holdings = BTreeMap::new();
    // トークン数量（smallest_units）: 価格×数量=価値 となるように設定
    // decimals=18 で rate() と一致させる
    holdings.insert(
        token_out("token-a"),
        TokenAmount::from_smallest_units(BigDecimal::from(5), 18),
    ); // price=100, value=500 NEAR
    holdings.insert(
        token_out("token-b"),
        TokenAmount::from_smallest_units(BigDecimal::from(10), 18),
    ); // price=50, value=500 NEAR

    WalletInfo {
        holdings,
        total_value: NearValue::from_near(BigDecimal::from(1000)), // 1000 NEAR
        cash_balance: NearValue::zero(),
    }
}

pub fn create_high_volatility_portfolio_data() -> super::PortfolioData {
    let mut tokens = create_sample_tokens();
    tokens.truncate(3); // 少数のトークンでテスト

    let mut predictions = BTreeMap::new();
    predictions.insert(token_out("token_a"), price(0.25));
    predictions.insert(token_out("token_b"), price(0.20));
    predictions.insert(token_out("token_c"), price(0.15));

    // 高ボラティリティの価格履歴を生成
    let historical_prices = create_high_volatility_price_history();

    super::PortfolioData {
        tokens,
        predictions,
        historical_prices,
        prediction_confidences: BTreeMap::new(),
    }
}

pub fn create_low_volatility_portfolio_data() -> super::PortfolioData {
    let mut tokens = create_sample_tokens();
    tokens.truncate(3);

    let mut predictions = BTreeMap::new();
    predictions.insert(token_out("token_a"), price(0.15));
    predictions.insert(token_out("token_b"), price(0.12));
    predictions.insert(token_out("token_c"), price(0.10));

    // 低ボラティリティの価格履歴を生成
    let historical_prices = create_low_volatility_price_history();

    super::PortfolioData {
        tokens,
        predictions,
        historical_prices,
        prediction_confidences: BTreeMap::new(),
    }
}

pub fn create_high_volatility_price_history() -> BTreeMap<TokenOutAccount, PriceHistory> {
    use chrono::{TimeDelta, TimeZone, Utc};

    let mut histories = BTreeMap::new();
    let tokens = ["token_a", "token_b", "token_c"];

    for token in tokens.iter() {
        let mut prices_vec = Vec::new();
        let mut p = 1000000000000000000i64; // 小さな価格単位

        // 30日間の高ボラティリティ価格データ
        for i in 0..30 {
            let timestamp =
                Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap() + TimeDelta::days(i);

            // ±15%の大きな変動を生成
            let volatility_factor = 1.0 + (i as f64 * 0.7).sin() * 0.15;
            p = ((p as f64 * volatility_factor) as i64).max(1);

            prices_vec.push(PricePoint {
                timestamp,
                price: TokenPrice::from_near_per_token(bigdecimal::BigDecimal::from(p)),
                volume: Some(bigdecimal::BigDecimal::from(1000000)), // ダミーボリューム
            });
        }

        let token_out_account: TokenOutAccount = token.parse().unwrap();
        let ph = PriceHistory {
            token: token_out_account.clone(),
            quote_token: token_in("wrap.near"), // ダミークォートトークン
            prices: prices_vec,
        };
        histories.insert(token_out_account, ph);
    }

    histories
}

pub fn create_low_volatility_price_history() -> BTreeMap<TokenOutAccount, PriceHistory> {
    use chrono::{TimeDelta, TimeZone, Utc};

    let mut histories = BTreeMap::new();
    let tokens = ["token_a", "token_b", "token_c"];

    for token in tokens.iter() {
        let mut prices_vec = Vec::new();
        let mut p = 1000000000000000000i64; // 小さな価格単位

        // 30日間の低ボラティリティ価格データ
        for i in 0..30 {
            let timestamp =
                Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap() + TimeDelta::days(i);

            // ±2%の小さな変動を生成
            let volatility_factor = 1.0 + (i as f64 * 0.3).sin() * 0.02;
            p = ((p as f64 * volatility_factor) as i64).max(1);

            prices_vec.push(PricePoint {
                timestamp,
                price: TokenPrice::from_near_per_token(bigdecimal::BigDecimal::from(p)),
                volume: Some(bigdecimal::BigDecimal::from(1000000)), // ダミーボリューム
            });
        }

        let token_out_account: TokenOutAccount = token.parse().unwrap();
        let ph = PriceHistory {
            token: token_out_account.clone(),
            quote_token: token_in("wrap.near"), // ダミークォートトークン
            prices: prices_vec,
        };
        histories.insert(token_out_account, ph);
    }

    histories
}

pub fn create_high_return_tokens() -> Vec<TokenData> {
    // 予測価格との整合性のために現在価格を設定:
    // - high_return_token: predicted = 0.50, +50% → current = 0.333
    // - medium_return_token: predicted = 0.30, +30% → current = 0.231
    // - stable_token: predicted = 0.10, +10% → current = 0.091
    vec![
        TokenData {
            symbol: token_out("high_return_token"),
            // current_price = 0.333 NEAR/token (50% リターンで 0.50 に)
            current_rate: ExchangeRate::from_price(
                &TokenPrice::from_near_per_token(
                    BigDecimal::from_f64(0.50 / 1.5).unwrap(), // 0.333
                ),
                24,
            ),
            historical_volatility: 0.40, // 40%ボラティリティ（高リスク・高リターン）
            liquidity_score: Some(0.9),
            market_cap: Some(cap(1000000)),
        },
        TokenData {
            symbol: token_out("medium_return_token"),
            // current_price = 0.231 NEAR/token (30% リターンで 0.30 に)
            current_rate: ExchangeRate::from_price(
                &TokenPrice::from_near_per_token(
                    BigDecimal::from_f64(0.30 / 1.3).unwrap(), // 0.231
                ),
                24,
            ),
            historical_volatility: 0.20, // 20%ボラティリティ
            liquidity_score: Some(0.8),
            market_cap: Some(cap(500000)),
        },
        TokenData {
            symbol: token_out("stable_token"),
            // current_price = 0.091 NEAR/token (10% リターンで 0.10 に)
            current_rate: ExchangeRate::from_price(
                &TokenPrice::from_near_per_token(
                    BigDecimal::from_f64(0.10 / 1.1).unwrap(), // 0.091
                ),
                24,
            ),
            historical_volatility: 0.10, // 10%ボラティリティ
            liquidity_score: Some(0.7),
            market_cap: Some(cap(2000000)),
        },
    ]
}

pub fn create_realistic_price_history() -> BTreeMap<TokenOutAccount, PriceHistory> {
    use chrono::{TimeDelta, TimeZone, Utc};

    let mut histories = BTreeMap::new();
    let token_configs = [
        ("high_return_token", 1000000000000000000i64, 0.03), // 3%日次成長期待
        ("medium_return_token", 500000000000000000i64, 0.02), // 2%日次成長期待
        ("stable_token", 2000000000000000000i64, 0.01),      // 1%日次成長期待
    ];

    for (token_name, initial_price, daily_growth) in token_configs.iter() {
        let mut prices_vec = Vec::new();
        let mut p = *initial_price;

        // 30日間の価格履歴
        for i in 0..30 {
            let timestamp =
                Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap() + TimeDelta::days(i);

            // トレンド成長 + ランダムノイズ
            let growth_factor = 1.0 + daily_growth + (i as f64 * 0.5).sin() * 0.005;
            p = ((p as f64 * growth_factor) as i64).max(1);

            prices_vec.push(PricePoint {
                timestamp,
                price: TokenPrice::from_near_per_token(bigdecimal::BigDecimal::from(p)),
                volume: Some(bigdecimal::BigDecimal::from(1000000)),
            });
        }

        let token_out_account: TokenOutAccount = token_name.parse().unwrap();
        let ph = PriceHistory {
            token: token_out_account.clone(),
            quote_token: token_in("wrap.near"),
            prices: prices_vec,
        };
        histories.insert(token_out_account, ph);
    }

    histories
}

pub fn calculate_expected_portfolio_return(
    weights: &PortfolioWeights,
    predictions: &BTreeMap<TokenOutAccount, TokenPrice>,
    tokens: &[TokenData],
) -> f64 {
    let mut total_return = 0.0;

    for token in tokens {
        if let Some(weight) = weights.weights.get(&token.symbol)
            && let Some(predicted_price) = predictions.get(&token.symbol)
        {
            // 現在価格から期待リターンを計算
            let current_price = token.current_rate.to_price();
            let expected_return = current_price.expected_return(predicted_price);
            total_return += weight.to_f64().unwrap_or(0.0) * expected_return;
        }
    }

    total_return
}

pub fn pow10(exp: u8) -> BigDecimal {
    BigDecimal::from_str(&format!("1{}", "0".repeat(exp as usize))).unwrap()
}

/// 元の calculate_current_weights の実装（BigDecimal直接計算版）
/// 計算結果の比較用
pub fn calculate_current_weights_original(tokens: &[TokenInfo], wallet: &WalletInfo) -> Vec<f64> {
    use bigdecimal::Zero;

    let mut weights = vec![0.0; tokens.len()];

    // NearValue から BigDecimal を直接取得（精度損失なし）
    let total_value_bd = wallet.total_value.as_bigdecimal().clone();

    for (i, token) in tokens.iter().enumerate() {
        if let Some(holding) = wallet.holdings.get(&token.symbol) {
            // TokenAmount から smallest_units を取得（精度損失なし）
            let holding_bd = holding.smallest_units().clone();

            // レートのBigDecimal表現を取得
            // raw_rate = tokens_smallest / NEAR
            let rate_bd = token.current_rate.raw_rate();

            // 価値を計算: holding / rate = tokens_smallest / (tokens_smallest/NEAR) = NEAR
            let value_near_bd = if rate_bd.is_zero() {
                BigDecimal::zero()
            } else {
                &holding_bd / rate_bd
            };

            // 重みを計算 (BigDecimal)
            if total_value_bd > 0 {
                let weight_bd = &value_near_bd / &total_value_bd;
                // 最終的にf64に変換（必要最小限のみ）
                weights[i] = weight_bd.to_string().parse::<f64>().unwrap_or(0.0);
            }
        }
    }

    weights
}

/// ランダムシード固定の合成リターンデータ生成（再現可能性保証）
pub fn generate_synthetic_returns(n: usize, t: usize, seed: u64) -> Vec<Vec<f64>> {
    let mut state = seed;
    (0..n)
        .map(|_| {
            (0..t)
                .map(|_| {
                    // 簡易 xorshift64
                    state ^= state << 13;
                    state ^= state >> 7;
                    state ^= state << 17;
                    let uniform = (state as f64) / (u64::MAX as f64);
                    (uniform - 0.5) * 0.1 // [-0.05, 0.05] の日次リターン
                })
                .collect()
        })
        .collect()
}
