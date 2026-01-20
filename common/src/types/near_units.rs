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

    /// f64 に変換（精度損失あり）
    pub fn to_f64(&self) -> TokenPriceF64 {
        TokenPriceF64(self.0.to_f64().unwrap_or(0.0))
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
        let current = self.to_f64().as_f64();
        let pred = predicted.to_f64().as_f64();
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
        TokenPrice(self.0 * BigDecimal::from_f64(scalar).unwrap_or_default())
    }
}

// スカラー (f64) × TokenPrice
impl Mul<TokenPrice> for f64 {
    type Output = TokenPrice;
    fn mul(self, price: TokenPrice) -> TokenPrice {
        TokenPrice(BigDecimal::from_f64(self).unwrap_or_default() * price.0)
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
        let divisor = BigDecimal::from_f64(scalar).unwrap_or_default();
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
// TokenPriceF64（価格）- f64 版
// =============================================================================

/// 価格（NEAR/token）- f64 版
///
/// シミュレーションやアルゴリズムで使用する高速版。
/// TokenPrice の f64 版。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct TokenPriceF64(f64);

impl TokenPriceF64 {
    /// ゼロ価格を作成
    pub fn zero() -> Self {
        TokenPriceF64(0.0)
    }

    /// NEAR/token の価格から作成
    ///
    /// # 引数
    /// - `near_per_token`: 1トークンあたりの NEAR 価格
    pub fn from_near_per_token(near_per_token: f64) -> Self {
        TokenPriceF64(near_per_token)
    }

    /// 内部の f64 を取得（計算用）
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// BigDecimal 版に変換（精度は回復しない）
    pub fn to_bigdecimal(&self) -> TokenPrice {
        TokenPrice(BigDecimal::from_f64(self.0).unwrap_or_default())
    }

    /// 価格がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

impl fmt::Display for TokenPriceF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Forward formatting options (precision, width, etc.) to inner f64
        fmt::Display::fmt(&self.0, f)
    }
}

// TokenPriceF64 同士の加算
impl Add for TokenPriceF64 {
    type Output = TokenPriceF64;
    fn add(self, other: TokenPriceF64) -> TokenPriceF64 {
        TokenPriceF64(self.0 + other.0)
    }
}

// TokenPriceF64 同士の減算
impl Sub for TokenPriceF64 {
    type Output = TokenPriceF64;
    fn sub(self, other: TokenPriceF64) -> TokenPriceF64 {
        TokenPriceF64(self.0 - other.0)
    }
}

// TokenPriceF64 同士の除算 → 比率を返す
impl Div for TokenPriceF64 {
    type Output = f64;
    fn div(self, other: TokenPriceF64) -> f64 {
        if other.0 == 0.0 {
            0.0
        } else {
            self.0 / other.0
        }
    }
}

// TokenPriceF64 × スカラー (f64)
impl Mul<f64> for TokenPriceF64 {
    type Output = TokenPriceF64;
    fn mul(self, scalar: f64) -> TokenPriceF64 {
        TokenPriceF64(self.0 * scalar)
    }
}

// スカラー (f64) × TokenPriceF64
impl Mul<TokenPriceF64> for f64 {
    type Output = TokenPriceF64;
    fn mul(self, price: TokenPriceF64) -> TokenPriceF64 {
        TokenPriceF64(self * price.0)
    }
}

// TokenPriceF64 / スカラー (f64)
impl Div<f64> for TokenPriceF64 {
    type Output = TokenPriceF64;
    fn div(self, scalar: f64) -> TokenPriceF64 {
        if scalar == 0.0 {
            TokenPriceF64::zero()
        } else {
            TokenPriceF64(self.0 / scalar)
        }
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

    /// f64 版に変換（精度損失あり）
    pub fn to_f64(&self) -> NearValueF64 {
        NearValueF64(self.0.to_f64().unwrap_or(0.0))
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
        NearValue(self.0 * BigDecimal::from_f64(rhs).unwrap_or_default())
    }
}

impl Mul<f64> for &NearValue {
    type Output = NearValue;
    fn mul(self, rhs: f64) -> NearValue {
        NearValue(&self.0 * BigDecimal::from_f64(rhs).unwrap_or_default())
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

// TokenPriceF64 × YoctoAmount (f64にキャストして計算)
impl Mul<YoctoAmount> for TokenPriceF64 {
    type Output = f64;
    fn mul(self, amount: YoctoAmount) -> f64 {
        self.0 * amount.0.to_f64().unwrap_or(0.0)
    }
}

// =============================================================================
// シミュレーション用 f64 型
// =============================================================================

/// トークン量（最小単位）- f64 版
///
/// シミュレーションで使用するトークン量。decimals 情報を保持し、
/// 異なる decimals のトークン間での誤った演算を防ぐ。
///
/// # 例
/// ```
/// use zaciraci_common::types::TokenAmountF64;
///
/// // USDT (decimals=6): 100 USDT = 100_000_000 smallest_units
/// let usdt = TokenAmountF64::from_smallest_units(100_000_000.0, 6);
///
/// // wNEAR (decimals=24): 1 wNEAR = 10^24 smallest_units
/// let wnear = TokenAmountF64::from_smallest_units(1e24, 24);
///
/// // whole token 単位で取得
/// assert!((usdt.to_whole() - 100.0).abs() < 0.001);
/// assert!((wnear.to_whole() - 1.0).abs() < 0.001);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TokenAmountF64 {
    /// 最小単位での量
    amount: f64,
    /// トークンの decimals
    decimals: u8,
}

impl TokenAmountF64 {
    /// ゼロ量を作成
    pub fn zero(decimals: u8) -> Self {
        TokenAmountF64 {
            amount: 0.0,
            decimals,
        }
    }

    /// smallest_units（最小単位）から作成
    pub fn from_smallest_units(smallest_units: f64, decimals: u8) -> Self {
        TokenAmountF64 {
            amount: smallest_units,
            decimals,
        }
    }

    /// whole tokens 単位から作成
    ///
    /// # 例
    /// ```
    /// use zaciraci_common::types::TokenAmountF64;
    ///
    /// // 100 USDT (decimals=6)
    /// let usdt = TokenAmountF64::from_whole_tokens(100.0, 6);
    /// assert!((usdt.as_f64() - 100_000_000.0).abs() < 0.001);
    ///
    /// // 1 wNEAR (decimals=24)
    /// let wnear = TokenAmountF64::from_whole_tokens(1.0, 24);
    /// assert!((wnear.as_f64() - 1e24).abs() < 1e18);
    /// ```
    pub fn from_whole_tokens(whole_tokens: f64, decimals: u8) -> Self {
        TokenAmountF64 {
            amount: whole_tokens * 10_f64.powi(decimals as i32),
            decimals,
        }
    }

    /// 内部の f64 を取得（計算用）
    pub fn as_f64(&self) -> f64 {
        self.amount
    }

    /// decimals を取得
    pub fn decimals(&self) -> u8 {
        self.decimals
    }

    /// whole token 単位に変換
    pub fn to_whole(&self) -> f64 {
        self.amount / 10_f64.powi(self.decimals as i32)
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.amount == 0.0
    }

    /// 量がゼロより大きいかどうか
    pub fn is_positive(&self) -> bool {
        self.amount > 0.0
    }

    /// 絶対値を返す
    pub fn abs(&self) -> Self {
        TokenAmountF64 {
            amount: self.amount.abs(),
            decimals: self.decimals,
        }
    }

    /// TokenAmount に変換（精度は回復しない）
    pub fn to_bigdecimal(&self) -> TokenAmount {
        TokenAmount::from_smallest_units(
            BigDecimal::from_f64(self.amount).unwrap_or_default(),
            self.decimals,
        )
    }
}

impl fmt::Display for TokenAmountF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (decimals={})", self.amount, self.decimals)
    }
}

impl PartialOrd for TokenAmountF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // decimals が異なる場合は whole 単位で比較
        if self.decimals == other.decimals {
            self.amount.partial_cmp(&other.amount)
        } else {
            self.to_whole().partial_cmp(&other.to_whole())
        }
    }
}

// TokenAmountF64 同士の加算（同じ decimals のみ）
impl Add for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn add(self, other: TokenAmountF64) -> TokenAmountF64 {
        debug_assert_eq!(
            self.decimals, other.decimals,
            "TokenAmountF64 addition requires same decimals: {} vs {}",
            self.decimals, other.decimals
        );
        TokenAmountF64 {
            amount: self.amount + other.amount,
            decimals: self.decimals,
        }
    }
}

// TokenAmountF64 同士の減算（同じ decimals のみ）
impl Sub for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn sub(self, other: TokenAmountF64) -> TokenAmountF64 {
        debug_assert_eq!(
            self.decimals, other.decimals,
            "TokenAmountF64 subtraction requires same decimals: {} vs {}",
            self.decimals, other.decimals
        );
        TokenAmountF64 {
            amount: self.amount - other.amount,
            decimals: self.decimals,
        }
    }
}

// TokenAmountF64 同士の除算 → 比率を返す（同じ decimals のみ）
impl Div for TokenAmountF64 {
    type Output = f64;
    fn div(self, other: TokenAmountF64) -> f64 {
        debug_assert_eq!(
            self.decimals, other.decimals,
            "TokenAmountF64 division requires same decimals: {} vs {}",
            self.decimals, other.decimals
        );
        if other.amount == 0.0 {
            0.0
        } else {
            self.amount / other.amount
        }
    }
}

// TokenAmountF64 × スカラー (f64)
impl Mul<f64> for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn mul(self, scalar: f64) -> TokenAmountF64 {
        TokenAmountF64 {
            amount: self.amount * scalar,
            decimals: self.decimals,
        }
    }
}

// スカラー (f64) × TokenAmountF64
impl Mul<TokenAmountF64> for f64 {
    type Output = TokenAmountF64;
    fn mul(self, amount: TokenAmountF64) -> TokenAmountF64 {
        TokenAmountF64 {
            amount: self * amount.amount,
            decimals: amount.decimals,
        }
    }
}

// TokenAmountF64 / スカラー (f64)
impl Div<f64> for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn div(self, scalar: f64) -> TokenAmountF64 {
        if scalar == 0.0 {
            TokenAmountF64::zero(self.decimals)
        } else {
            TokenAmountF64 {
                amount: self.amount / scalar,
                decimals: self.decimals,
            }
        }
    }
}

/// 金額（yoctoNEAR 単位）- f64 版
///
/// シミュレーションで使用する金額。TokenAmountF64 × TokenPriceF64 の結果。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct YoctoValueF64(f64);

/// 1 NEAR = 10^24 yoctoNEAR (f64 版)
const YOCTO_PER_NEAR_F64: f64 = 1e24;

impl YoctoValueF64 {
    /// ゼロ金額を作成
    pub fn zero() -> Self {
        YoctoValueF64(0.0)
    }

    /// yoctoNEAR 単位の金額から作成
    pub fn from_yocto(yocto: f64) -> Self {
        YoctoValueF64(yocto)
    }

    /// 内部の f64 を取得（計算用）
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// NEAR 単位に変換
    pub fn to_near(&self) -> NearValueF64 {
        NearValueF64(self.0 / YOCTO_PER_NEAR_F64)
    }

    /// 金額がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// 金額がゼロより大きいかどうか
    pub fn is_positive(&self) -> bool {
        self.0 > 0.0
    }

    /// BigDecimal 版（YoctoValue）に変換（精度は回復しない）
    pub fn to_bigdecimal(&self) -> YoctoValue {
        YoctoValue::from_yocto(BigDecimal::from_f64(self.0).unwrap_or_default())
    }

    /// 価格で割ってトークン量に変換
    ///
    /// # 計算
    /// - yoctoNEAR / (NEAR/token) → token (smallest_units)
    /// - result = yoctoNEAR / 10^24 / price × 10^decimals
    ///
    /// # 引数
    /// - `price`: トークン価格（NEAR/token）
    /// - `decimals`: 結果のトークンの decimals
    pub fn to_amount(&self, price: TokenPriceF64, decimals: u8) -> TokenAmountF64 {
        if price.is_zero() {
            return TokenAmountF64::zero(decimals);
        }
        // yoctoNEAR → NEAR → whole tokens → smallest_units
        let near_value = self.0 / YOCTO_PER_NEAR_F64;
        let whole_tokens = near_value / price.as_f64();
        let smallest_units = whole_tokens * 10_f64.powi(decimals as i32);
        TokenAmountF64::from_smallest_units(smallest_units, decimals)
    }
}

impl fmt::Display for YoctoValueF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Forward formatting options (precision, width, etc.) to inner f64
        fmt::Display::fmt(&self.0, f)
    }
}

// YoctoValueF64 同士の加算
impl Add for YoctoValueF64 {
    type Output = YoctoValueF64;
    fn add(self, other: YoctoValueF64) -> YoctoValueF64 {
        YoctoValueF64(self.0 + other.0)
    }
}

// YoctoValueF64 同士の減算
impl Sub for YoctoValueF64 {
    type Output = YoctoValueF64;
    fn sub(self, other: YoctoValueF64) -> YoctoValueF64 {
        YoctoValueF64(self.0 - other.0)
    }
}

// YoctoValueF64 同士の除算 → 比率を返す
impl Div for YoctoValueF64 {
    type Output = f64;
    fn div(self, other: YoctoValueF64) -> f64 {
        if other.0 == 0.0 {
            0.0
        } else {
            self.0 / other.0
        }
    }
}

// YoctoValueF64 × スカラー (f64)
impl Mul<f64> for YoctoValueF64 {
    type Output = YoctoValueF64;
    fn mul(self, scalar: f64) -> YoctoValueF64 {
        YoctoValueF64(self.0 * scalar)
    }
}

// スカラー (f64) × YoctoValueF64
impl Mul<YoctoValueF64> for f64 {
    type Output = YoctoValueF64;
    fn mul(self, value: YoctoValueF64) -> YoctoValueF64 {
        YoctoValueF64(self * value.0)
    }
}

// YoctoValueF64 - スカラー (f64)
impl Sub<f64> for YoctoValueF64 {
    type Output = YoctoValueF64;
    fn sub(self, scalar: f64) -> YoctoValueF64 {
        YoctoValueF64(self.0 - scalar)
    }
}

// YoctoValueF64 / TokenPriceF64 = TokenAmountF64
// 注意: decimals 情報が必要なため、to_amount() メソッドを使用することを推奨
impl Div<TokenPriceF64> for YoctoValueF64 {
    type Output = TokenAmountF64;
    fn div(self, price: TokenPriceF64) -> TokenAmountF64 {
        // デフォルト decimals=24 で作成（後方互換性）
        self.to_amount(price, 24)
    }
}

// YoctoValueF64 / TokenAmountF64 = TokenPriceF64
impl Div<TokenAmountF64> for YoctoValueF64 {
    type Output = TokenPriceF64;
    fn div(self, amount: TokenAmountF64) -> TokenPriceF64 {
        // whole token 単位に変換してから計算
        let whole_amount = amount.to_whole();
        if whole_amount == 0.0 {
            TokenPriceF64::zero()
        } else {
            // yoctoNEAR / token → NEAR/token に変換
            TokenPriceF64::from_near_per_token(self.to_near().as_f64() / whole_amount)
        }
    }
}

/// 金額（NEAR 単位）- f64 版
///
/// ユーザー表示用の金額。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct NearValueF64(f64);

impl NearValueF64 {
    /// ゼロ金額を作成
    pub fn zero() -> Self {
        NearValueF64(0.0)
    }

    /// NEAR 単位の金額から作成
    pub fn from_near(near: f64) -> Self {
        NearValueF64(near)
    }

    /// 内部の f64 を取得（計算用）
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// yoctoNEAR 単位に変換
    pub fn to_yocto(&self) -> YoctoValueF64 {
        YoctoValueF64(self.0 * YOCTO_PER_NEAR_F64)
    }

    /// 金額がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// 金額がゼロより大きいかどうか
    pub fn is_positive(&self) -> bool {
        self.0 > 0.0
    }

    /// 絶対値を返す
    pub fn abs(&self) -> Self {
        NearValueF64(self.0.abs())
    }
}

impl fmt::Display for NearValueF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} NEAR", self.0)
    }
}

// NearValueF64 同士の加算
impl Add for NearValueF64 {
    type Output = NearValueF64;
    fn add(self, other: NearValueF64) -> NearValueF64 {
        NearValueF64(self.0 + other.0)
    }
}

// NearValueF64 同士の減算
impl Sub for NearValueF64 {
    type Output = NearValueF64;
    fn sub(self, other: NearValueF64) -> NearValueF64 {
        NearValueF64(self.0 - other.0)
    }
}

// NearValueF64 同士の除算 → 比率を返す
impl Div for NearValueF64 {
    type Output = f64;
    fn div(self, other: NearValueF64) -> f64 {
        if other.0 == 0.0 {
            0.0
        } else {
            self.0 / other.0
        }
    }
}

// NearValueF64 × スカラー (f64)
impl Mul<f64> for NearValueF64 {
    type Output = NearValueF64;
    fn mul(self, scalar: f64) -> NearValueF64 {
        NearValueF64(self.0 * scalar)
    }
}

// スカラー (f64) × NearValueF64
impl Mul<NearValueF64> for f64 {
    type Output = NearValueF64;
    fn mul(self, value: NearValueF64) -> NearValueF64 {
        NearValueF64(self * value.0)
    }
}

// NearValueF64 ÷ スカラー (f64)
impl Div<f64> for NearValueF64 {
    type Output = NearValueF64;
    fn div(self, scalar: f64) -> NearValueF64 {
        if scalar == 0.0 {
            NearValueF64(0.0)
        } else {
            NearValueF64(self.0 / scalar)
        }
    }
}

// NearValueF64 + スカラー (f64)
impl Add<f64> for NearValueF64 {
    type Output = NearValueF64;
    fn add(self, scalar: f64) -> NearValueF64 {
        NearValueF64(self.0 + scalar)
    }
}

// NearValueF64 - スカラー (f64)
impl Sub<f64> for NearValueF64 {
    type Output = NearValueF64;
    fn sub(self, scalar: f64) -> NearValueF64 {
        NearValueF64(self.0 - scalar)
    }
}

// =============================================================================
// f64 版の Price × Amount = Value の演算
// =============================================================================

// TokenAmountF64 × TokenPriceF64 = YoctoValueF64
// 計算: amount (smallest_units) / 10^decimals × price (NEAR/token) × 10^24 = yoctoNEAR
impl Mul<TokenPriceF64> for TokenAmountF64 {
    type Output = YoctoValueF64;
    fn mul(self, price: TokenPriceF64) -> YoctoValueF64 {
        // smallest_units → whole tokens → NEAR → yoctoNEAR
        let whole_tokens = self.to_whole();
        let near_value = whole_tokens * price.as_f64();
        NearValueF64::from_near(near_value).to_yocto()
    }
}

// TokenPriceF64 × TokenAmountF64 = YoctoValueF64
impl Mul<TokenAmountF64> for TokenPriceF64 {
    type Output = YoctoValueF64;
    fn mul(self, amount: TokenAmountF64) -> YoctoValueF64 {
        amount * self // 可換なので上の実装に委譲
    }
}

#[cfg(test)]
mod tests;
