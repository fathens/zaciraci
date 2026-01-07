//! NEAR/yoctoNEAR 単位の型安全な型定義
//!
//! このモジュールは価格、量、金額を型安全に扱うための型を提供します。
//!
//! ## 概念
//!
//! - **Price**: 1トークンあたりのNEAR価値（無次元比率）
//! - **Amount**: トークンの数量（yoctoNEAR または NEAR 単位）
//! - **Value**: Price × Amount の結果（yoctoNEAR または NEAR 単位）
//!
//! ## 型間の演算
//!
//! ```text
//! Price × YoctoAmount = YoctoValue
//! Price × NearAmount = NearValue
//! YoctoValue / Price = YoctoAmount
//! YoctoValue / YoctoAmount = Price
//! ```

use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Div, Mul, Sub};

/// 1 NEAR = 10^24 yoctoNEAR
pub(crate) const YOCTO_PER_NEAR: u128 = 1_000_000_000_000_000_000_000_000;

// =============================================================================
// Price（価格）- 無次元比率
// =============================================================================

/// 価格（無次元比率）- BigDecimal 版
///
/// 1トークンあたり何NEARかを表す比率。
/// 単位を持たないため、yoctoNEAR/NEAR の区別は不要。
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Price(BigDecimal);

impl Price {
    /// ゼロ価格を作成
    pub fn zero() -> Self {
        Price(BigDecimal::zero())
    }

    /// BigDecimal から Price を作成
    pub fn new(value: BigDecimal) -> Self {
        Price(value)
    }

    /// 内部の BigDecimal を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// BigDecimal に変換
    pub fn into_bigdecimal(self) -> BigDecimal {
        self.0
    }

    /// f64 版に変換（精度損失あり）
    pub fn to_f64(&self) -> PriceF64 {
        PriceF64(self.0.to_f64().unwrap_or(0.0))
    }

    /// 価格がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Price 同士の加算
impl Add for Price {
    type Output = Price;
    fn add(self, other: Price) -> Price {
        Price(self.0 + other.0)
    }
}

impl Add<&Price> for Price {
    type Output = Price;
    fn add(self, other: &Price) -> Price {
        Price(self.0 + &other.0)
    }
}

// Price 同士の減算
impl Sub for Price {
    type Output = Price;
    fn sub(self, other: Price) -> Price {
        Price(self.0 - other.0)
    }
}

impl Sub<&Price> for Price {
    type Output = Price;
    fn sub(self, other: &Price) -> Price {
        Price(self.0 - &other.0)
    }
}

impl Sub for &Price {
    type Output = Price;
    fn sub(self, other: &Price) -> Price {
        Price(&self.0 - &other.0)
    }
}

// Price 同士の除算 → 比率を返す
impl Div for Price {
    type Output = BigDecimal;
    fn div(self, other: Price) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / other.0
        }
    }
}

impl Div<&Price> for Price {
    type Output = BigDecimal;
    fn div(self, other: &Price) -> BigDecimal {
        if other.0.is_zero() {
            BigDecimal::zero()
        } else {
            self.0 / &other.0
        }
    }
}

// Price × スカラー (f64)
impl Mul<f64> for Price {
    type Output = Price;
    fn mul(self, scalar: f64) -> Price {
        Price(self.0 * BigDecimal::from_f64(scalar).unwrap_or_default())
    }
}

// スカラー (f64) × Price
impl Mul<Price> for f64 {
    type Output = Price;
    fn mul(self, price: Price) -> Price {
        Price(BigDecimal::from_f64(self).unwrap_or_default() * price.0)
    }
}

// Price × スカラー (BigDecimal)
impl Mul<BigDecimal> for Price {
    type Output = Price;
    fn mul(self, scalar: BigDecimal) -> Price {
        Price(self.0 * scalar)
    }
}

// Price / スカラー (f64)
impl Div<f64> for Price {
    type Output = Price;
    fn div(self, scalar: f64) -> Price {
        let divisor = BigDecimal::from_f64(scalar).unwrap_or_default();
        if divisor.is_zero() {
            Price::zero()
        } else {
            Price(self.0 / divisor)
        }
    }
}

// =============================================================================
// PriceF64（価格）- f64 版
// =============================================================================

/// 価格（無次元比率）- f64 版
///
/// シミュレーションやアルゴリズムで使用する高速版。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PriceF64(f64);

impl PriceF64 {
    /// ゼロ価格を作成
    pub fn zero() -> Self {
        PriceF64(0.0)
    }

    /// f64 から PriceF64 を作成
    pub fn new(value: f64) -> Self {
        PriceF64(value)
    }

    /// 内部の f64 を取得
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// BigDecimal 版に変換（精度は回復しない）
    pub fn to_bigdecimal(&self) -> Price {
        Price(BigDecimal::from_f64(self.0).unwrap_or_default())
    }

    /// 価格がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

impl fmt::Display for PriceF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Forward formatting options (precision, width, etc.) to inner f64
        fmt::Display::fmt(&self.0, f)
    }
}

// PriceF64 同士の加算
impl Add for PriceF64 {
    type Output = PriceF64;
    fn add(self, other: PriceF64) -> PriceF64 {
        PriceF64(self.0 + other.0)
    }
}

// PriceF64 同士の減算
impl Sub for PriceF64 {
    type Output = PriceF64;
    fn sub(self, other: PriceF64) -> PriceF64 {
        PriceF64(self.0 - other.0)
    }
}

// PriceF64 同士の除算 → 比率を返す
impl Div for PriceF64 {
    type Output = f64;
    fn div(self, other: PriceF64) -> f64 {
        if other.0 == 0.0 {
            0.0
        } else {
            self.0 / other.0
        }
    }
}

// PriceF64 × スカラー (f64)
impl Mul<f64> for PriceF64 {
    type Output = PriceF64;
    fn mul(self, scalar: f64) -> PriceF64 {
        PriceF64(self.0 * scalar)
    }
}

// スカラー (f64) × PriceF64
impl Mul<PriceF64> for f64 {
    type Output = PriceF64;
    fn mul(self, price: PriceF64) -> PriceF64 {
        PriceF64(self * price.0)
    }
}

// PriceF64 / スカラー (f64)
impl Div<f64> for PriceF64 {
    type Output = PriceF64;
    fn div(self, scalar: f64) -> PriceF64 {
        if scalar == 0.0 {
            PriceF64::zero()
        } else {
            PriceF64(self.0 / scalar)
        }
    }
}

// =============================================================================
// YoctoAmount（量）- yoctoNEAR 単位
// =============================================================================

/// 量（yoctoNEAR 単位）
///
/// トークン残高やスワップ量を表す。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct YoctoAmount(u128);

impl YoctoAmount {
    /// ゼロ量を作成
    pub fn zero() -> Self {
        YoctoAmount(0)
    }

    /// u128 から YoctoAmount を作成
    pub fn new(value: u128) -> Self {
        YoctoAmount(value)
    }

    /// 内部の u128 を取得
    pub fn as_u128(&self) -> u128 {
        self.0
    }

    /// NEAR 単位に変換
    pub fn to_near(&self) -> NearAmount {
        NearAmount(BigDecimal::from(self.0) / BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0
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
        YoctoAmount(self.0.saturating_add(other.0))
    }
}

// YoctoAmount 同士の減算
impl Sub for YoctoAmount {
    type Output = YoctoAmount;
    fn sub(self, other: YoctoAmount) -> YoctoAmount {
        YoctoAmount(self.0.saturating_sub(other.0))
    }
}

// YoctoAmount 同士の除算 → 比率を返す
impl Div for YoctoAmount {
    type Output = BigDecimal;
    fn div(self, other: YoctoAmount) -> BigDecimal {
        if other.0 == 0 {
            BigDecimal::zero()
        } else {
            BigDecimal::from(self.0) / BigDecimal::from(other.0)
        }
    }
}

// YoctoAmount × スカラー (u128)
impl Mul<u128> for YoctoAmount {
    type Output = YoctoAmount;
    fn mul(self, scalar: u128) -> YoctoAmount {
        YoctoAmount(self.0.saturating_mul(scalar))
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

    /// BigDecimal から NearAmount を作成
    pub fn new(value: BigDecimal) -> Self {
        NearAmount(value)
    }

    /// 内部の BigDecimal を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// yoctoNEAR 単位に変換
    pub fn to_yocto(&self) -> YoctoAmount {
        let yocto = &self.0 * BigDecimal::from(YOCTO_PER_NEAR);
        YoctoAmount(yocto.to_u128().unwrap_or(0))
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
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

    /// BigDecimal から YoctoValue を作成
    pub fn new(value: BigDecimal) -> Self {
        YoctoValue(value)
    }

    /// 内部の BigDecimal を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// BigDecimal に変換
    pub fn into_bigdecimal(self) -> BigDecimal {
        self.0
    }

    /// NEAR 単位に変換
    pub fn to_near(&self) -> NearValue {
        NearValue(&self.0 / BigDecimal::from(YOCTO_PER_NEAR))
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

// YoctoValue / Price = YoctoAmount
impl Div<Price> for YoctoValue {
    type Output = YoctoAmount;
    fn div(self, price: Price) -> YoctoAmount {
        if price.0.is_zero() {
            YoctoAmount::zero()
        } else {
            let result = self.0 / price.0;
            YoctoAmount(result.to_u128().unwrap_or(0))
        }
    }
}

// YoctoValue / YoctoAmount = Price
impl Div<YoctoAmount> for YoctoValue {
    type Output = Price;
    fn div(self, amount: YoctoAmount) -> Price {
        if amount.0 == 0 {
            Price::zero()
        } else {
            Price(self.0 / BigDecimal::from(amount.0))
        }
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

    /// BigDecimal から NearValue を作成
    pub fn new(value: BigDecimal) -> Self {
        NearValue(value)
    }

    /// 内部の BigDecimal を取得
    pub fn as_bigdecimal(&self) -> &BigDecimal {
        &self.0
    }

    /// BigDecimal に変換
    pub fn into_bigdecimal(self) -> BigDecimal {
        self.0
    }

    /// yoctoNEAR 単位に変換
    pub fn to_yocto(&self) -> YoctoValue {
        YoctoValue(&self.0 * BigDecimal::from(YOCTO_PER_NEAR))
    }

    /// 金額がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
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

// NearValue / Price = NearAmount
impl Div<Price> for NearValue {
    type Output = NearAmount;
    fn div(self, price: Price) -> NearAmount {
        if price.0.is_zero() {
            NearAmount::zero()
        } else {
            NearAmount(self.0 / price.0)
        }
    }
}

// NearValue / NearAmount = Price
impl Div<NearAmount> for NearValue {
    type Output = Price;
    fn div(self, amount: NearAmount) -> Price {
        if amount.0.is_zero() {
            Price::zero()
        } else {
            Price(self.0 / amount.0)
        }
    }
}

// =============================================================================
// Price × Amount = Value の演算
// =============================================================================

// Price × YoctoAmount = YoctoValue
impl Mul<YoctoAmount> for Price {
    type Output = YoctoValue;
    fn mul(self, amount: YoctoAmount) -> YoctoValue {
        YoctoValue(self.0 * BigDecimal::from(amount.0))
    }
}

// YoctoAmount × Price = YoctoValue
impl Mul<Price> for YoctoAmount {
    type Output = YoctoValue;
    fn mul(self, price: Price) -> YoctoValue {
        YoctoValue(BigDecimal::from(self.0) * price.0)
    }
}

// Price × NearAmount = NearValue
impl Mul<NearAmount> for Price {
    type Output = NearValue;
    fn mul(self, amount: NearAmount) -> NearValue {
        NearValue(self.0 * amount.0)
    }
}

// NearAmount × Price = NearValue
impl Mul<Price> for NearAmount {
    type Output = NearValue;
    fn mul(self, price: Price) -> NearValue {
        NearValue(self.0 * price.0)
    }
}

// PriceF64 × YoctoAmount (f64にキャストして計算)
impl Mul<YoctoAmount> for PriceF64 {
    type Output = f64;
    fn mul(self, amount: YoctoAmount) -> f64 {
        self.0 * (amount.0 as f64)
    }
}

// =============================================================================
// シミュレーション用 f64 型
// =============================================================================

/// トークン量（最小単位）- f64 版
///
/// シミュレーションで使用するトークン量。decimals=24 の場合、1 token = 10^24 smallest_unit。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct TokenAmountF64(f64);

impl TokenAmountF64 {
    /// ゼロ量を作成
    pub fn zero() -> Self {
        TokenAmountF64(0.0)
    }

    /// f64 から TokenAmountF64 を作成
    pub fn new(value: f64) -> Self {
        TokenAmountF64(value)
    }

    /// 内部の f64 を取得
    pub fn as_f64(&self) -> f64 {
        self.0
    }

    /// 量がゼロかどうか
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// 量がゼロより大きいかどうか
    pub fn is_positive(&self) -> bool {
        self.0 > 0.0
    }

    /// BigDecimal に変換（精度は回復しない）
    pub fn to_bigdecimal(&self) -> BigDecimal {
        BigDecimal::from_f64(self.0).unwrap_or_default()
    }
}

impl fmt::Display for TokenAmountF64 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Forward formatting options (precision, width, etc.) to inner f64
        fmt::Display::fmt(&self.0, f)
    }
}

// TokenAmountF64 同士の加算
impl Add for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn add(self, other: TokenAmountF64) -> TokenAmountF64 {
        TokenAmountF64(self.0 + other.0)
    }
}

// TokenAmountF64 同士の減算
impl Sub for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn sub(self, other: TokenAmountF64) -> TokenAmountF64 {
        TokenAmountF64(self.0 - other.0)
    }
}

// TokenAmountF64 同士の除算 → 比率を返す
impl Div for TokenAmountF64 {
    type Output = f64;
    fn div(self, other: TokenAmountF64) -> f64 {
        if other.0 == 0.0 {
            0.0
        } else {
            self.0 / other.0
        }
    }
}

// TokenAmountF64 × スカラー (f64)
impl Mul<f64> for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn mul(self, scalar: f64) -> TokenAmountF64 {
        TokenAmountF64(self.0 * scalar)
    }
}

// スカラー (f64) × TokenAmountF64
impl Mul<TokenAmountF64> for f64 {
    type Output = TokenAmountF64;
    fn mul(self, amount: TokenAmountF64) -> TokenAmountF64 {
        TokenAmountF64(self * amount.0)
    }
}

// TokenAmountF64 / スカラー (f64)
impl Div<f64> for TokenAmountF64 {
    type Output = TokenAmountF64;
    fn div(self, scalar: f64) -> TokenAmountF64 {
        if scalar == 0.0 {
            TokenAmountF64::zero()
        } else {
            TokenAmountF64(self.0 / scalar)
        }
    }
}

/// 金額（yoctoNEAR 単位）- f64 版
///
/// シミュレーションで使用する金額。TokenAmountF64 × PriceF64 の結果。
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Default, Serialize, Deserialize)]
pub struct YoctoValueF64(f64);

/// 1 NEAR = 10^24 yoctoNEAR (f64 版)
const YOCTO_PER_NEAR_F64: f64 = 1e24;

impl YoctoValueF64 {
    /// ゼロ金額を作成
    pub fn zero() -> Self {
        YoctoValueF64(0.0)
    }

    /// f64 から YoctoValueF64 を作成
    pub fn new(value: f64) -> Self {
        YoctoValueF64(value)
    }

    /// 内部の f64 を取得
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
        YoctoValue::new(BigDecimal::from_f64(self.0).unwrap_or_default())
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

// YoctoValueF64 / PriceF64 = TokenAmountF64
impl Div<PriceF64> for YoctoValueF64 {
    type Output = TokenAmountF64;
    fn div(self, price: PriceF64) -> TokenAmountF64 {
        if price.0 == 0.0 {
            TokenAmountF64::zero()
        } else {
            TokenAmountF64(self.0 / price.0)
        }
    }
}

// YoctoValueF64 / TokenAmountF64 = PriceF64
impl Div<TokenAmountF64> for YoctoValueF64 {
    type Output = PriceF64;
    fn div(self, amount: TokenAmountF64) -> PriceF64 {
        if amount.0 == 0.0 {
            PriceF64::zero()
        } else {
            PriceF64(self.0 / amount.0)
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

    /// f64 から NearValueF64 を作成
    pub fn new(value: f64) -> Self {
        NearValueF64(value)
    }

    /// 内部の f64 を取得
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

// =============================================================================
// f64 版の Price × Amount = Value の演算
// =============================================================================

// TokenAmountF64 × PriceF64 = YoctoValueF64
impl Mul<PriceF64> for TokenAmountF64 {
    type Output = YoctoValueF64;
    fn mul(self, price: PriceF64) -> YoctoValueF64 {
        YoctoValueF64(self.0 * price.0)
    }
}

// PriceF64 × TokenAmountF64 = YoctoValueF64
impl Mul<TokenAmountF64> for PriceF64 {
    type Output = YoctoValueF64;
    fn mul(self, amount: TokenAmountF64) -> YoctoValueF64 {
        YoctoValueF64(self.0 * amount.0)
    }
}

#[cfg(test)]
mod tests;
