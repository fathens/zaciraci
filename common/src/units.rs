use bigdecimal::BigDecimal;
use std::str::FromStr;

/// NEAR単位の変換ユーティリティ
/// 内部では全てyoctoNEAR単位で処理し、表示やユーザー入力時のみNEAR単位を使用
pub struct Units;

impl Units {
    /// 1 NEAR = 10^24 yoctoNEAR
    pub const YOCTO_PER_NEAR: &'static str = "1000000000000000000000000";

    /// yoctoNEAR から NEAR に変換
    pub fn yocto_to_near(yocto: &BigDecimal) -> BigDecimal {
        let divisor = BigDecimal::from_str(Self::YOCTO_PER_NEAR).unwrap();
        yocto / divisor
    }

    /// NEAR から yoctoNEAR に変換
    pub fn near_to_yocto(near: &BigDecimal) -> BigDecimal {
        let multiplier = BigDecimal::from_str(Self::YOCTO_PER_NEAR).unwrap();
        near * multiplier
    }

    /// f64 (yoctoNEAR) から NEAR (f64) に変換
    pub fn yocto_f64_to_near_f64(yocto: f64) -> f64 {
        yocto / 1e24
    }

    /// NEAR (f64) から yoctoNEAR (f64) に変換
    pub fn near_f64_to_yocto_f64(near: f64) -> f64 {
        near * 1e24
    }

    /// BigDecimal (yoctoNEAR) から 表示用NEAR文字列に変換
    pub fn format_near(yocto: &BigDecimal) -> String {
        let near = Self::yocto_to_near(yocto);
        format!("{:.6}", near)
    }

    /// f64 (yoctoNEAR) から 表示用NEAR文字列に変換
    pub fn format_near_f64(yocto: f64) -> String {
        let near = Self::yocto_f64_to_near_f64(yocto);
        format!("{:.6}", near)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yocto_to_near() {
        let yocto = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 1 NEAR
        let near = Units::yocto_to_near(&yocto);
        assert_eq!(near, BigDecimal::from_str("1").unwrap());
    }

    #[test]
    fn test_near_to_yocto() {
        let near = BigDecimal::from_str("1").unwrap(); // 1 NEAR
        let yocto = Units::near_to_yocto(&near);
        assert_eq!(
            yocto,
            BigDecimal::from_str("1000000000000000000000000").unwrap()
        );
    }

    #[test]
    fn test_f64_conversions() {
        let yocto = 1e24; // 1 NEAR in yoctoNEAR
        let near = Units::yocto_f64_to_near_f64(yocto);
        assert_eq!(near, 1.0);

        let back_to_yocto = Units::near_f64_to_yocto_f64(near);
        assert_eq!(back_to_yocto, yocto);
    }

    #[test]
    fn test_format_functions() {
        let yocto = BigDecimal::from_str("1500000000000000000000000").unwrap(); // 1.5 NEAR
        let formatted = Units::format_near(&yocto);
        assert_eq!(formatted, "1.500000");

        let yocto_f64 = 1.5e24;
        let formatted_f64 = Units::format_near_f64(yocto_f64);
        assert_eq!(formatted_f64, "1.500000");
    }
}
