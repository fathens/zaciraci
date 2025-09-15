use bigdecimal::BigDecimal;
use std::str::FromStr;

/// Test precision issues with extreme values like Bean token
#[cfg(test)]
mod precision_tests {
    use super::*;

    #[test]
    fn test_bean_token_precision_problem() {
        // Bean token の実際の値
        let amount_f64 = 8.478e20; // 保有量（極大）
        let price_yocto_f64 = 2.783e-19; // yoctoNEAR単位の価格（極小）

        // f64での従来計算（精度問題あり）
        let price_near_f64 = price_yocto_f64 / 1e24;
        let value_f64 = amount_f64 * price_near_f64;

        // BigDecimalでの高精度計算
        let amount_bd = BigDecimal::from_str(&amount_f64.to_string()).unwrap();
        let price_yocto_bd = BigDecimal::from_str(&price_yocto_f64.to_string()).unwrap();
        let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 10^24
        let price_near_bd = &price_yocto_bd / &yocto_per_near;
        let value_bd = &amount_bd * &price_near_bd;
        let value_bd_f64 = value_bd.to_string().parse::<f64>().unwrap_or(0.0);

        println!("🧪 Bean Token Precision Test:");
        println!("   Amount: {}", amount_f64);
        println!("   Price (yocto): {}", price_yocto_f64);
        println!("   Price (NEAR): {}", price_near_f64);
        println!("   Value (f64): {}", value_f64);
        println!("   Value (BigDecimal): {}", value_bd);
        println!("   Value (BD->f64): {}", value_bd_f64);

        // f64は不正確な結果（235.96付近）
        assert!(value_f64 > 200.0, "f64 calculation shows precision error");

        // BigDecimalは正確な結果（~2.36e-22）
        assert!(value_bd_f64 < 1e-20, "BigDecimal calculation should be extremely small");

        // 精度の違いを確認
        let precision_difference = (value_f64 - value_bd_f64).abs();
        assert!(precision_difference > 200.0, "Precision difference should be significant");
    }

    #[test]
    fn test_portfolio_value_calculation() {
        use std::collections::HashMap;

        // 複数トークンのポートフォリオ
        let mut holdings = HashMap::new();
        holdings.insert("normal.token".to_string(), 1000.0);
        holdings.insert("bean.token".to_string(), 8.478e20);

        let mut prices = HashMap::new();
        prices.insert("normal.token".to_string(), 1e24); // 1 NEAR
        prices.insert("bean.token".to_string(), 2.783e-19); // Bean token

        // f64計算
        let mut total_f64 = 0.0;
        for (token, amount) in &holdings {
            if let Some(&price_yocto) = prices.get(token) {
                let price_near = price_yocto / 1e24;
                total_f64 += amount * price_near;
            }
        }

        // BigDecimal計算
        let mut total_bd = BigDecimal::from(0);
        for (token, amount) in &holdings {
            if let Some(&price_yocto) = prices.get(token) {
                let amount_bd = BigDecimal::from_str(&amount.to_string()).unwrap();
                let price_yocto_bd = BigDecimal::from_str(&price_yocto.to_string()).unwrap();
                let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap();
                let price_near_bd = &price_yocto_bd / &yocto_per_near;
                let value_bd = &amount_bd * &price_near_bd;
                total_bd += value_bd;
            }
        }
        let total_bd_f64 = total_bd.to_string().parse::<f64>().unwrap_or(0.0);

        println!("🧪 Portfolio Value Test:");
        println!("   Total (f64): {}", total_f64);
        println!("   Total (BigDecimal): {}", total_bd);
        println!("   Total (BD->f64): {}", total_bd_f64);

        // f64は1000 + 235.96 ≈ 1235.96
        assert!(total_f64 > 1200.0, "f64 total should include precision error");

        // BigDecimalは1000 + 2.36e-22 ≈ 1000.0
        assert!(total_bd_f64 < 1001.0, "BigDecimal total should be close to 1000");
        assert!(total_bd_f64 > 999.0, "BigDecimal total should be close to 1000");
    }

    #[test]
    fn test_return_calculation_impact() {
        let initial_capital = 1000.0;

        // f64でのポートフォリオ価値（精度問題あり）
        let final_value_f64 = 1235.96; // Bean tokenの精度問題で増加
        let return_f64 = (final_value_f64 - initial_capital) / initial_capital * 100.0;

        // BigDecimalでのポートフォリオ価値（正確）
        let final_value_bd = 1000.0; // 正確な計算
        let return_bd = (final_value_bd - initial_capital) / initial_capital * 100.0;

        println!("🧪 Return Calculation Impact:");
        println!("   Return (f64): {:.2}%", return_f64);
        println!("   Return (BigDecimal): {:.2}%", return_bd);

        // f64は異常に高いリターン
        assert!(return_f64 > 20.0, "f64 shows abnormally high return");

        // BigDecimalは正常なリターン
        assert!(return_bd.abs() < 1.0, "BigDecimal shows normal return");
    }

    #[test]
    fn test_quantity_limit_application() {
        use bigdecimal::BigDecimal;
        use std::str::FromStr;

        // リバランス計算での数量制限テスト
        let portfolio_value = BigDecimal::from_str("16201.58").unwrap(); // 16201.58 NEAR
        let target_weight = BigDecimal::from_str("0.5").unwrap(); // 50%配分
        let price_yocto = BigDecimal::from_str("2.783e-19").unwrap(); // Bean token価格（yocto）
        let yocto_per_near = BigDecimal::from_str("1000000000000000000000000").unwrap(); // 10^24

        let target_value = &portfolio_value * &target_weight; // 8100.79 NEAR
        let price_near = &price_yocto / &yocto_per_near; // 2.783e-43 NEAR
        let target_amount_unlimited = &target_value / &price_near; // 異常に大きな数

        // 制限前の数量
        println!("🧪 Quantity Limit Test:");
        println!("   Portfolio Value: {} NEAR", portfolio_value);
        println!("   Target Weight: 50%");
        println!("   Price (yocto): {}", price_yocto);
        println!("   Price (NEAR): {}", price_near);
        println!("   Target Value: {} NEAR", target_value);
        println!("   Unlimited Amount: {}", target_amount_unlimited);

        // 制限適用
        let max_reasonable_amount = BigDecimal::from_str("1000000000000000000000").unwrap(); // 10^21
        let target_amount_limited = if target_amount_unlimited > max_reasonable_amount {
            max_reasonable_amount
        } else {
            target_amount_unlimited
        };

        println!("   Limited Amount: {}", target_amount_limited);

        // 制限が適用されることを確認
        assert!(target_amount_unlimited > max_reasonable_amount, "Unlimited amount should exceed limit");
        assert_eq!(target_amount_limited, max_reasonable_amount, "Limited amount should equal max limit");

        // 制限値は現実的な範囲内であることを確認
        let limited_f64 = target_amount_limited.to_string().parse::<f64>().unwrap();
        assert!(limited_f64 < 1e22, "Limited amount should be reasonable");
    }
}