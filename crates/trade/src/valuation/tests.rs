use super::*;
use bigdecimal::BigDecimal;
use std::str::FromStr;

/// テスト用の RateProvider 実装
struct MockRateProvider {
    rates: BTreeMap<TokenOutAccount, ExchangeRate>,
}

impl MockRateProvider {
    fn new() -> Self {
        Self {
            rates: BTreeMap::new(),
        }
    }

    fn with_rate(mut self, token: &str, raw_rate: &str, decimals: u8) -> Self {
        let token_account: TokenAccount = token.parse().unwrap();
        let token_out: TokenOutAccount = token_account.into();
        let rate = ExchangeRate::from_raw_rate(BigDecimal::from_str(raw_rate).unwrap(), decimals);
        self.rates.insert(token_out, rate);
        self
    }
}

impl RateProvider for MockRateProvider {
    async fn get_rate(&self, token: &TokenOutAccount) -> Result<Option<ExchangeRate>> {
        Ok(self.rates.get(token).cloned())
    }
}

/// wnear のみのポートフォリオ → そのまま NEAR 換算
#[tokio::test]
async fn test_wnear_only() {
    let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
    let mut holdings = BTreeMap::new();
    // 10 wNEAR = 10_000000000000000000000000 yocto (decimals=24)
    holdings.insert(
        wnear.clone(),
        TokenAmount::from_smallest_units(
            BigDecimal::from_str("10000000000000000000000000").unwrap(),
            24,
        ),
    );

    let provider = MockRateProvider::new();
    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();

    // 10 wNEAR = 10 NEAR
    assert_eq!(value.to_string(), "10 NEAR");
}

/// 空ポートフォリオ → 0 NEAR
#[tokio::test]
async fn test_empty_portfolio() {
    let holdings = BTreeMap::new();
    let provider = MockRateProvider::new();
    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();
    assert_eq!(value, NearValue::zero());
}

/// ゼロ残高のトークンはスキップされる
#[tokio::test]
async fn test_zero_balance_skipped() {
    let token: TokenAccount = "usdt.tether-token.near".parse().unwrap();
    let mut holdings = BTreeMap::new();
    holdings.insert(
        token,
        TokenAmount::from_smallest_units(BigDecimal::from(0), 6),
    );

    let provider = MockRateProvider::new();
    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();
    assert_eq!(value, NearValue::zero());
}

/// レートが存在しないトークン → スキップ（warn ログ）、エラーにはならない
#[tokio::test]
async fn test_missing_rate_skipped() {
    let token: TokenAccount = "unknown-token.near".parse().unwrap();
    let mut holdings = BTreeMap::new();
    holdings.insert(
        token,
        TokenAmount::from_smallest_units(BigDecimal::from(1000000), 6),
    );

    let provider = MockRateProvider::new();
    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();
    assert_eq!(value, NearValue::zero());
}

/// 通常トークン → レートで NEAR 換算
#[tokio::test]
async fn test_token_with_rate() {
    // USDT: decimals=6, raw_rate=5000000 (= 5 * 10^6 smallest_units / NEAR)
    // つまり 1 NEAR = 5 USDT → 1 USDT = 0.2 NEAR
    let token: TokenAccount = "usdt.tether-token.near".parse().unwrap();
    let mut holdings = BTreeMap::new();
    // 10 USDT = 10_000000 smallest_units
    holdings.insert(
        token,
        TokenAmount::from_smallest_units(BigDecimal::from(10_000_000), 6),
    );

    let provider = MockRateProvider::new().with_rate("usdt.tether-token.near", "5000000", 6);

    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();
    // 10 USDT / (5000000 / 10^6) = 10 / 5 = 2 NEAR
    assert_eq!(value.to_string(), "2 NEAR");
}

/// wnear + 通常トークンの混合ポートフォリオ
#[tokio::test]
async fn test_mixed_portfolio() {
    let wnear = &*blockchain::ref_finance::token_account::WNEAR_TOKEN;
    let usdt: TokenAccount = "usdt.tether-token.near".parse().unwrap();

    let mut holdings = BTreeMap::new();
    // 5 wNEAR
    holdings.insert(
        wnear.clone(),
        TokenAmount::from_smallest_units(
            BigDecimal::from_str("5000000000000000000000000").unwrap(),
            24,
        ),
    );
    // 10 USDT (1 USDT = 0.2 NEAR → 10 USDT = 2 NEAR)
    holdings.insert(
        usdt,
        TokenAmount::from_smallest_units(BigDecimal::from(10_000_000), 6),
    );

    let provider = MockRateProvider::new().with_rate("usdt.tether-token.near", "5000000", 6);

    let value = calculate_portfolio_value(&holdings, &provider)
        .await
        .unwrap();
    // 5 NEAR + 2 NEAR = 7 NEAR
    assert_eq!(value.to_string(), "7 NEAR");
}
