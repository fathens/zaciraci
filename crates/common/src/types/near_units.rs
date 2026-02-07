//! NEAR/yoctoNEAR 単位の型安全な型定義
//!
//! このモジュールは価格、量、金額を型安全に扱うための型を提供します。
//!
//! ## 概念
//!
//! - **TokenPrice**: 1トークンあたりのNEAR価値（無次元比率）
//! - **Amount**: トークンの数量（yoctoNEAR または NEAR 単位）
//! - **Value**: TokenPrice × Amount の結果（yoctoNEAR または NEAR 単位）
//!
//! ## 型間の演算
//!
//! ```text
//! TokenPrice × YoctoAmount = YoctoValue
//! TokenPrice × NearAmount = NearValue
//! YoctoValue / TokenPrice = YoctoAmount
//! YoctoValue / YoctoAmount = TokenPrice
//! ```
//!
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};
use std::str::FromStr;

use super::token_types::{ExchangeRate, TokenAmount};

/// f64 → BigDecimal 変換。NaN/Infinity の場合はゼロを返し debug_assert で検出。
fn bigdecimal_from_f64_safe(value: f64) -> BigDecimal {
    debug_assert!(
        value.is_finite(),
        "bigdecimal_from_f64_safe: non-finite f64: {value}"
    );
    BigDecimal::from_f64(value).unwrap_or_default()
}

/// BigDecimal → f64 変換。非有限値（Inf 含む）の場合はゼロを返し debug_assert で検出。
///
/// BigDecimal::to_f64() はオーバーフロー時に None ではなく Some(Inf) を返すため、
/// Some 側でも is_finite() チェックが必要。
fn bigdecimal_to_f64_safe(value: &BigDecimal) -> f64 {
    match value.to_f64() {
        Some(f) if f.is_finite() => f,
        other => {
            debug_assert!(
                false,
                "bigdecimal_to_f64_safe: non-finite conversion for {value}: {other:?}"
            );
            0.0
        }
    }
}

/// 1 NEAR = 10^24 yoctoNEAR
pub(crate) const YOCTO_PER_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

// =============================================================================
// TokenPrice（価格）- 無次元比率
// =============================================================================

/// トークン価格（NEAR / token）- BigDecimal 版
///
/// 1トークンあたり何NEARかを表す比率。
/// 単位を持たないため、yoctoNEAR/NEAR の区別は不要。
///
/// - `TokenPrice` が大きい = トークンが高い
/// - `TokenPrice` が小さい = トークンが安い
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TokenPrice(BigDecimal);

impl TokenPrice {
    /// ゼロ価格を作成
    pub fn zero() -> Self {
        TokenPrice(BigDecimal::zero())
    }

    /// BigDecimal から TokenPrice を作成
    ///
    /// 値は「1トークンあたり何NEAR」を表す。
    /// 主にキャッシュされた価格データの読み込み用。
    pub fn from_near_per_token(near_per_token: BigDecimal) -> Self {
        TokenPrice(near_per_token)
    }

    /// 内部の BigDecimal を取得（計算用）
    ///
    /// 注意: この値を他の型のコンストラクタに渡さないこと。
    /// 型変換には専用のメソッドを使う。
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// 価格がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// 期待リターンを計算
    ///
    /// ```text
    /// return = (predicted - current) / current
    /// ```
    ///
    /// # 注意
    ///
    /// `ExchangeRate` から直接リターンを計算すると符号が逆になる。
    /// `TokenPrice` を使えばこの混乱を防げる。
    pub fn expected_return(&self, predicted: &TokenPrice) -> f64 {
        let current = bigdecimal_to_f64_safe(&self.0);
        let pred = bigdecimal_to_f64_safe(&predicted.0);
        if current == 0.0 {
            return 0.0;
        }
        (pred - current) / current
    }
}

impl fmt::Display for TokenPrice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// TokenPrice 同士の加算
impl Add for TokenPrice {
    type Output = TokenPrice;
    fn add(self, other: TokenPrice) -> TokenPrice {
        TokenPrice(self.0 + other.0)
    }
}

impl Add<&TokenPrice> for TokenPrice {
    type Output = TokenPrice;
    fn add(self, other: &TokenPrice) -> TokenPrice {
        TokenPrice(self.0 + &other.0)
    }
}

// TokenPrice 同士の減算
impl Sub for TokenPrice {
    type Output = TokenPrice;
    fn sub(self, other: TokenPrice) -> TokenPrice {
        TokenPrice(self.0 - other.0)
    }
}

impl Sub<&TokenPrice> for TokenPrice {
    type Output = TokenPrice;
    fn sub(self, other: &TokenPrice) -> TokenPrice {
        TokenPrice(self.0 - &other.0)
    }
}

impl Sub for &TokenPrice {
    type Output = TokenPrice;
    fn sub(self, other: &TokenPrice) -> TokenPrice {
        TokenPrice(&self.0 - &other.0)
    }
}

// TokenPrice 同士の除算 → 比率を返す
impl Div for TokenPrice {
    type Output = BigDecimal;
    fn div(self, other: TokenPrice) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

impl Div<&TokenPrice> for TokenPrice {
    type Output = BigDecimal;
    fn div(self, other: &TokenPrice) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / &other.0
        }
    }
}

// TokenPrice × スカラー (f64)
impl Mul<f64> for TokenPrice {
    type Output = TokenPrice;
    fn mul(self, scalar: f64) -> TokenPrice {
        TokenPrice(self.0 * bigdecimal_from_f64_safe(scalar))
    }
}

// スカラー (f64) × TokenPrice
impl Mul<TokenPrice> for f64 {
    type Output = TokenPrice;
    fn mul(self, price: TokenPrice) -> TokenPrice {
        TokenPrice(bigdecimal_from_f64_safe(self) * price.0)
    }
}

// TokenPrice × スカラー (BigDecimal)
impl Mul<BigDecimal> for TokenPrice {
    type Output = TokenPrice;
    fn mul(self, scalar: BigDecimal) -> TokenPrice {
        TokenPrice(self.0 * scalar)
    }
}

// TokenPrice / スカラー (BigDecimal)
impl Div<BigDecimal> for TokenPrice {
    type Output = TokenPrice;
    fn div(self, scalar: BigDecimal) -> TokenPrice {
        if scalar.is_zero() {
            TokenPrice::zero()
        } else {
            TokenPrice(self.0 / scalar)
        }
    }
}

// TokenPrice / スカラー (f64)
impl Div<f64> for TokenPrice {
    type Output = TokenPrice;
    fn div(self, scalar: f64) -> TokenPrice {
        let divisor = bigdecimal_from_f64_safe(scalar);
        if divisor.is_zero() {
            TokenPrice::zero()
        } else {
            TokenPrice(self.0 / divisor)
        }
    }
}

// TokenPrice / スカラー (i64) - 平均計算用
impl Div<i64> for TokenPrice {
    type Output = TokenPrice;
    fn div(self, scalar: i64) -> TokenPrice {
        if scalar == 0 {
            TokenPrice::zero()
        } else {
            TokenPrice(self.0 / BigDecimal::from(scalar))
        }
    }
}

impl Div<i64> for &TokenPrice {
    type Output = TokenPrice;
    fn div(self, scalar: i64) -> TokenPrice {
        if scalar == 0 {
            TokenPrice::zero()
        } else {
            TokenPrice(&self.0 / BigDecimal::from(scalar))
        }
    }
}

// TokenPrice の Sum trait（統計計算用）
impl std::iter::Sum for TokenPrice {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(TokenPrice::zero(), |acc, x| acc + x)
    }
}

impl<'a> std::iter::Sum<&'a TokenPrice> for TokenPrice {
    fn sum<I: Iterator<Item = &'a Self>>(iter: I) -> Self {
        iter.fold(TokenPrice::zero(), |acc, x| acc + x)
    }
}

// =============================================================================
// YoctoAmount（量）- yoctoNEAR 単位
// =============================================================================

/// 量（yoctoNEAR 単位）- BigDecimal 版
///
/// トークン残高やスワップ量を表す。
///
/// # 内部表現
///
/// BigDecimal を使用しているため、計算途中の精度損失がない。
/// 最終的にブロックチェーンに送信する際は `to_u128()` で整数部を取得する。
///
/// # 例
///
/// ```ignore
/// use common::types::{YoctoValue, TokenPrice, YoctoAmount};
/// use bigdecimal::BigDecimal;
///
/// let value = YoctoValue::from_yocto(BigDecimal::from(1001));
/// let price = TokenPrice::from_f64(2.0);
/// let amount: YoctoAmount = value / price;
/// // 1001 / 2 = 500.5（精度を保持）
/// assert_eq!(amount.as_bigdecimal(), &BigDecimal::from_str("500.5").unwrap());
/// // ブロックチェーン用に整数部を取得
/// assert_eq!(amount.to_u128(), 500);
/// ```
///
/// 詳細は `tests::test_yocto_amount_truncation_behavior` を参照。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YoctoAmount(BigDecimal);

impl YoctoAmount {
    /// ゼロ量を作成
    pub fn zero() -> Self {
        YoctoAmount(BigDecimal::zero())
    }

    /// u128 から YoctoAmount を作成
    pub fn from_u128(value: u128) -> Self {
        YoctoAmount(BigDecimal::from(value))
    }

    /// 内部の BigDecimal への参照を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// 整数部を u128 として取得（切り捨て）
    ///
    /// ブロックチェーンに送信する際に使用する。
    /// yoctoNEAR より小さい単位は存在しないため、整数部のみを取得する。
    pub fn to_u128(&self) -> u128 {
        self.0.to_u128().unwrap_or(0)
    }

    /// NEAR 単位に変換
    pub fn to_near(&self) -> NearAmount {
        NearAmount(&self.0 / BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// YoctoValue（価値）に変換
    ///
    /// NEAR は native トークンなので、数量と価値は同じ値になる。
    /// ポートフォリオ価値計算など、数量を価値として扱う際に使用する。
    pub fn to_value(&self) -> YoctoValue {
        YoctoValue(self.0.clone())
    }

    /// TokenAmount に変換
    ///
    /// yoctoNEAR は NEAR/wNEAR の最小単位（decimals=24）なので、
    /// そのまま TokenAmount に変換できる。
    ///
    /// # 用途
    ///
    /// wNEAR ⇔ NEAR の変換時など、decimals が 24 で固定されている
    /// 場合に使用する。get_token_decimals() を呼び出す必要がない。
    pub fn to_token_amount(&self) -> TokenAmount {
        TokenAmount::from_smallest_units(self.0.clone(), 24)
    }
}

impl fmt::Display for YoctoAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// YoctoAmount 同士の加算
impl Add for YoctoAmount {
    type Output = YoctoAmount;
    fn add(self, other: YoctoAmount) -> YoctoAmount {
        YoctoAmount(self.0 + other.0)
    }
}

impl Add<&YoctoAmount> for YoctoAmount {
    type Output = YoctoAmount;
    fn add(self, other: &YoctoAmount) -> YoctoAmount {
        YoctoAmount(self.0 + &other.0)
    }
}

// YoctoAmount 同士の減算
impl Sub for YoctoAmount {
    type Output = YoctoAmount;
    fn sub(self, other: YoctoAmount) -> YoctoAmount {
        let result = &self.0 - &other.0;
        // 負の値にならないようにする（ブロックチェーンの量は常に非負）
        if result < BigDecimal::zero() {
            YoctoAmount::zero()
        } else {
            YoctoAmount(result)
        }
    }
}

impl Sub<&YoctoAmount> for YoctoAmount {
    type Output = YoctoAmount;
    fn sub(self, other: &YoctoAmount) -> YoctoAmount {
        let result = self.0 - &other.0;
        if result < BigDecimal::zero() {
            YoctoAmount::zero()
        } else {
            YoctoAmount(result)
        }
    }
}

// YoctoAmount 同士の除算 → 比率を返す
impl Div for YoctoAmount {
    type Output = BigDecimal;
    fn div(self, other: YoctoAmount) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

// YoctoAmount × スカラー (u128)
impl Mul<u128> for YoctoAmount {
    type Output = YoctoAmount;
    fn mul(self, scalar: u128) -> YoctoAmount {
        YoctoAmount(self.0 * BigDecimal::from(scalar))
    }
}

// YoctoAmount × スカラー (BigDecimal)
impl Mul<BigDecimal> for YoctoAmount {
    type Output = YoctoAmount;
    fn mul(self, scalar: BigDecimal) -> YoctoAmount {
        YoctoAmount(self.0 * scalar)
    }
}

// =============================================================================
// NearAmount（量）- NEAR 単位
// =============================================================================

/// 量（NEAR 単位）
///
/// ユーザー表示用の量。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NearAmount(BigDecimal);

impl NearAmount {
    /// ゼロ量を作成
    pub fn zero() -> Self {
        NearAmount(BigDecimal::zero())
    }

    /// 内部の BigDecimal を取得（計算用）
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// yoctoNEAR 単位に変換
    pub fn to_yocto(&self) -> YoctoAmount {
        YoctoAmount(&self.0 * BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// 整数部を i64 として取得（切り捨て）
    ///
    /// PostgreSQL BIGINT (signed i64) との互換性を保つため i64 を使用。
    /// NearAmount 最大値 ≈ 3.4 × 10^14 は i64 最大値より十分小さいため安全。
    pub fn to_i64(&self) -> i64 {
        self.0.to_i64().unwrap_or(0)
    }
}

impl FromStr for NearAmount {
    type Err = bigdecimal::ParseBigDecimalError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.parse::<BigDecimal>()?;
        Ok(NearAmount(value))
    }
}

impl fmt::Display for NearAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} NEAR", self.0)
    }
}

// NearAmount 同士の加算
impl Add for NearAmount {
    type Output = NearAmount;
    fn add(self, other: NearAmount) -> NearAmount {
        NearAmount(self.0 + other.0)
    }
}

impl Add<&NearAmount> for NearAmount {
    type Output = NearAmount;
    fn add(self, other: &NearAmount) -> NearAmount {
        NearAmount(self.0 + &other.0)
    }
}

// NearAmount 同士の減算
impl Sub for NearAmount {
    type Output = NearAmount;
    fn sub(self, other: NearAmount) -> NearAmount {
        NearAmount(self.0 - other.0)
    }
}

impl Sub<&NearAmount> for NearAmount {
    type Output = NearAmount;
    fn sub(self, other: &NearAmount) -> NearAmount {
        NearAmount(self.0 - &other.0)
    }
}

// NearAmount 同士の除算 → 比率を返す
impl Div for NearAmount {
    type Output = BigDecimal;
    fn div(self, other: NearAmount) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

// =============================================================================
// YoctoValue（金額）- yoctoNEAR 単位
// =============================================================================

/// 金額（yoctoNEAR 単位）
///
/// Price × Amount の結果。ポートフォリオ評価額など。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YoctoValue(BigDecimal);

impl YoctoValue {
    /// ゼロ金額を作成
    pub fn zero() -> Self {
        YoctoValue(BigDecimal::zero())
    }

    /// yoctoNEAR 単位の金額から作成
    pub fn from_yocto(yocto: BigDecimal) -> Self {
        YoctoValue(yocto)
    }

    /// 内部の BigDecimal を取得（計算用）
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// NEAR 単位に変換
    pub fn to_near(&self) -> NearValue {
        NearValue(&self.0 / BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// YoctoAmount（数量）に変換
    ///
    /// NEAR は native トークンなので、価値と数量は同じ値になる。
    /// 送金時など、価値を数量として扱う際に使用する。
    pub fn to_amount(&self) -> YoctoAmount {
        YoctoAmount(self.0.clone())
    }

    /// 金額がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl fmt::Display for YoctoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// YoctoValue 同士の加算
impl Add for YoctoValue {
    type Output = YoctoValue;
    fn add(self, other: YoctoValue) -> YoctoValue {
        YoctoValue(self.0 + other.0)
    }
}

impl Add<&YoctoValue> for YoctoValue {
    type Output = YoctoValue;
    fn add(self, other: &YoctoValue) -> YoctoValue {
        YoctoValue(self.0 + &other.0)
    }
}

// YoctoValue 同士の減算
impl Sub for YoctoValue {
    type Output = YoctoValue;
    fn sub(self, other: YoctoValue) -> YoctoValue {
        YoctoValue(self.0 - other.0)
    }
}

impl Sub<&YoctoValue> for YoctoValue {
    type Output = YoctoValue;
    fn sub(self, other: &YoctoValue) -> YoctoValue {
        YoctoValue(self.0 - &other.0)
    }
}

// YoctoValue 同士の除算 → 比率を返す
impl Div for YoctoValue {
    type Output = BigDecimal;
    fn div(self, other: YoctoValue) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

impl Div<&YoctoValue> for YoctoValue {
    type Output = BigDecimal;
    fn div(self, other: &YoctoValue) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / &other.0
        }
    }
}

// &YoctoValue - &YoctoValue → YoctoValue (参照同士の減算)
impl Sub<&YoctoValue> for &YoctoValue {
    type Output = YoctoValue;
    fn sub(self, other: &YoctoValue) -> YoctoValue {
        YoctoValue((&self.0) - (&other.0))
    }
}

// &YoctoValue / &YoctoValue → BigDecimal (参照同士の除算、比率を返す)
impl Div<&YoctoValue> for &YoctoValue {
    type Output = BigDecimal;
    fn div(self, other: &YoctoValue) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            (&self.0) / (&other.0)
        }
    }
}

// YoctoValue / TokenPrice = YoctoAmount
impl Div<TokenPrice> for YoctoValue {
    type Output = YoctoAmount;
    fn div(self, price: TokenPrice) -> YoctoAmount {
        if price.0.is_zero() {
            YoctoAmount::zero()
        } else {
            YoctoAmount(self.0 / price.0)
        }
    }
}

// YoctoValue / YoctoAmount = TokenPrice
impl Div<YoctoAmount> for YoctoValue {
    type Output = TokenPrice;
    fn div(self, amount: YoctoAmount) -> TokenPrice {
        if amount.0.is_zero() {
            TokenPrice::zero()
        } else {
            TokenPrice(self.0 / amount.0)
        }
    }
}

// &YoctoValue * BigDecimal = YoctoValue（スカラー乗算）
impl Mul<BigDecimal> for &YoctoValue {
    type Output = YoctoValue;
    fn mul(self, scalar: BigDecimal) -> YoctoValue {
        YoctoValue(&self.0 * scalar)
    }
}

// &YoctoValue * &BigDecimal = YoctoValue（スカラー乗算）
impl Mul<&BigDecimal> for &YoctoValue {
    type Output = YoctoValue;
    fn mul(self, scalar: &BigDecimal) -> YoctoValue {
        YoctoValue(&self.0 * scalar)
    }
}

// =============================================================================
// NearValue（金額）- NEAR 単位
// =============================================================================

/// 金額（NEAR 単位）
///
/// ユーザー表示用の金額。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NearValue(BigDecimal);

impl NearValue {
    /// ゼロ金額を作成
    pub fn zero() -> Self {
        NearValue(BigDecimal::zero())
    }

    /// 1 NEAR を作成
    pub fn one() -> Self {
        NearValue(BigDecimal::from(1))
    }

    /// NEAR 単位の金額から作成
    pub fn from_near(near: BigDecimal) -> Self {
        NearValue(near)
    }

    /// 内部の BigDecimal を取得（計算用）
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// yoctoNEAR 単位に変換
    pub fn to_yocto(&self) -> YoctoValue {
        YoctoValue(&self.0 * BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// 金額がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    /// 絶対値を取得
    pub fn abs(&self) -> NearValue {
        NearValue(self.0.abs())
    }
}

impl fmt::Display for NearValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} NEAR", self.0)
    }
}

// NearValue 同士の加算
impl Add for NearValue {
    type Output = NearValue;
    fn add(self, other: NearValue) -> NearValue {
        NearValue(self.0 + other.0)
    }
}

impl Add<&NearValue> for NearValue {
    type Output = NearValue;
    fn add(self, other: &NearValue) -> NearValue {
        NearValue(self.0 + &other.0)
    }
}

// NearValue 同士の減算
impl Sub for NearValue {
    type Output = NearValue;
    fn sub(self, other: NearValue) -> NearValue {
        NearValue(self.0 - other.0)
    }
}

impl Sub<&NearValue> for NearValue {
    type Output = NearValue;
    fn sub(self, other: &NearValue) -> NearValue {
        NearValue(self.0 - &other.0)
    }
}

impl Sub<&NearValue> for &NearValue {
    type Output = NearValue;
    fn sub(self, other: &NearValue) -> NearValue {
        NearValue(&self.0 - &other.0)
    }
}

// NearValue × f64 → NearValue（ウェイト計算用）
impl Mul<f64> for NearValue {
    type Output = NearValue;
    fn mul(self, rhs: f64) -> NearValue {
        NearValue(self.0 * bigdecimal_from_f64_safe(rhs))
    }
}

impl Mul<f64> for &NearValue {
    type Output = NearValue;
    fn mul(self, rhs: f64) -> NearValue {
        NearValue(&self.0 * bigdecimal_from_f64_safe(rhs))
    }
}

impl Mul<&BigDecimal> for &NearValue {
    type Output = NearValue;
    fn mul(self, rhs: &BigDecimal) -> NearValue {
        NearValue(&self.0 * rhs)
    }
}

// NearValue の符号反転
impl Neg for NearValue {
    type Output = NearValue;
    fn neg(self) -> NearValue {
        NearValue(-self.0)
    }
}

impl Neg for &NearValue {
    type Output = NearValue;
    fn neg(self) -> NearValue {
        NearValue(-self.0.clone())
    }
}

// NearValue 同士の除算 → 比率を返す
impl Div for NearValue {
    type Output = BigDecimal;
    fn div(self, other: NearValue) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

// &NearValue / &NearValue = BigDecimal（参照版）
impl Div<&NearValue> for &NearValue {
    type Output = BigDecimal;
    fn div(self, other: &NearValue) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            &self.0 / &other.0
        }
    }
}

// NearValue / TokenPrice = NearAmount
impl Div<TokenPrice> for NearValue {
    type Output = NearAmount;
    fn div(self, price: TokenPrice) -> NearAmount {
        if price.0.is_zero() {
            NearAmount::zero()
        } else {
            NearAmount(self.0 / price.0)
        }
    }
}

// NearValue / NearAmount = TokenPrice
impl Div<NearAmount> for NearValue {
    type Output = TokenPrice;
    fn div(self, amount: NearAmount) -> TokenPrice {
        if amount.0.is_zero() {
            TokenPrice::zero()
        } else {
            TokenPrice(self.0 / amount.0)
        }
    }
}

// &NearValue * &ExchangeRate = TokenAmount
// NEAR価値をトークン数量に変換（リバランス時の売却量計算用）
//
// 次元分析:
//   ExchangeRate.raw_rate = smallest_units / NEAR
//   NEAR × (smallest_units/NEAR) = smallest_units
impl Mul<&ExchangeRate> for &NearValue {
    type Output = TokenAmount;

    fn mul(self, rhs: &ExchangeRate) -> Self::Output {
        let smallest_units = if rhs.is_zero() {
            BigDecimal::zero()
        } else {
            &self.0 * rhs.raw_rate()
        };
        TokenAmount::from_smallest_units(smallest_units, rhs.decimals())
    }
}

// =============================================================================
// TokenPrice × Amount = Value の演算
// =============================================================================

// TokenPrice × YoctoAmount = YoctoValue
impl Mul<YoctoAmount> for TokenPrice {
    type Output = YoctoValue;
    fn mul(self, amount: YoctoAmount) -> YoctoValue {
        YoctoValue(self.0 * amount.0)
    }
}

// YoctoAmount × TokenPrice = YoctoValue
impl Mul<TokenPrice> for YoctoAmount {
    type Output = YoctoValue;
    fn mul(self, price: TokenPrice) -> YoctoValue {
        YoctoValue(self.0 * price.0)
    }
}

// &YoctoAmount × TokenPrice = YoctoValue（参照版）
impl Mul<TokenPrice> for &YoctoAmount {
    type Output = YoctoValue;
    fn mul(self, price: TokenPrice) -> YoctoValue {
        YoctoValue(&self.0 * &price.0)
    }
}

// TokenPrice × NearAmount = NearValue
impl Mul<NearAmount> for TokenPrice {
    type Output = NearValue;
    fn mul(self, amount: NearAmount) -> NearValue {
        NearValue(self.0 * amount.0)
    }
}

// NearAmount × TokenPrice = NearValue
impl Mul<TokenPrice> for NearAmount {
    type Output = NearValue;
    fn mul(self, price: TokenPrice) -> NearValue {
        NearValue(self.0 * price.0)
    }
}

#[cfg(test)]
mod tests;
