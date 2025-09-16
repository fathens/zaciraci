#[cfg(test)]
#[allow(clippy::module_inception)]
mod cost_fix_test {
    use super::super::types::FeeModel;
    use super::super::utils::{
        calculate_trading_cost, calculate_trading_cost_by_value,
        calculate_trading_cost_by_value_yocto, calculate_trading_cost_by_value_yocto_bd,
    };
    use bigdecimal::{BigDecimal, FromPrimitive};

    #[test]
    fn test_cost_calculation_comparison() {
        // 実際のシミュレーション条件を再現
        let token_amount = 5.45e20; // nearai.aidols.nearの保有量
        let token_price_yocto = 166759.9203717577; // yoctoNEAR単位
        let token_price_near = common::units::Units::yocto_f64_to_near_f64(token_price_yocto);
        let trade_value = token_amount * token_price_near; // NEAR建ての取引価値

        println!("🧮 Trading Cost Calculation Comparison:");
        println!("   Token amount: {:.2e}", token_amount);
        println!("   Token price: {:.2e} NEAR", token_price_near);
        println!("   Trade value: {:.6} NEAR", trade_value);

        // 古い方法（数量ベース）
        let old_cost = calculate_trading_cost(
            token_amount,
            &FeeModel::Realistic,
            0.01, // 1% slippage
            0.01, // gas cost
        );

        // 新しい方法（価値ベース）
        let new_cost_value = calculate_trading_cost_by_value(
            trade_value,
            &FeeModel::Realistic,
            0.01, // 1% slippage
            0.01, // gas cost
        );

        // yoctoNEAR統一方法
        let trade_value_yocto = token_amount * token_price_yocto;
        let gas_cost_yocto = common::units::Units::near_f64_to_yocto_f64(0.01);
        let yocto_cost_value = calculate_trading_cost_by_value_yocto(
            trade_value_yocto,
            &FeeModel::Realistic,
            0.01, // 1% slippage
            gas_cost_yocto,
        );

        // 新しい方法をトークン数量で表現
        let new_cost_tokens = if token_price_near > 0.0 {
            new_cost_value / token_price_near
        } else {
            0.0
        };

        // yoctoNEAR方法をトークン数量で表現
        let yocto_cost_tokens = if token_price_yocto > 0.0 {
            yocto_cost_value / token_price_yocto
        } else {
            0.0
        };

        // BigDecimal統一方法（最高精度）
        let trade_value_yocto_bd = BigDecimal::from_f64(token_amount).unwrap_or_default()
            * BigDecimal::from_f64(token_price_yocto).unwrap_or_default();
        let gas_cost_yocto_bd = BigDecimal::from_f64(gas_cost_yocto).unwrap_or_default();
        let slippage_rate_bd = BigDecimal::from_f64(0.01).unwrap_or_default();
        let bd_cost_value = calculate_trading_cost_by_value_yocto_bd(
            &trade_value_yocto_bd,
            &FeeModel::Realistic,
            &slippage_rate_bd,
            &gas_cost_yocto_bd,
        );

        // BigDecimal方法をトークン数量で表現
        let bd_cost_tokens = if token_price_yocto > 0.0 {
            (&bd_cost_value / BigDecimal::from_f64(token_price_yocto).unwrap_or_default())
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0)
        } else {
            0.0
        };

        println!("\n   📊 Cost Comparison:");
        println!("   Old method (amount-based):");
        println!("     Cost in tokens: {:.2e}", old_cost);
        println!("     Cost in NEAR: {:.6}", old_cost * token_price_near);

        println!("   New method (value-based):");
        println!("     Cost in NEAR: {:.6}", new_cost_value);
        println!("     Cost in tokens: {:.2e}", new_cost_tokens);

        println!("   yoctoNEAR unified method:");
        println!("     Cost in yoctoNEAR: {:.2e}", yocto_cost_value);
        println!("     Cost in tokens: {:.2e}", yocto_cost_tokens);
        println!(
            "     Cost in NEAR: {:.6}",
            common::units::Units::yocto_f64_to_near_f64(yocto_cost_value)
        );

        println!("   BigDecimal precision method:");
        println!("     Cost in yoctoNEAR: {}", bd_cost_value);
        println!("     Cost in tokens: {:.2e}", bd_cost_tokens);
        println!(
            "     Cost in NEAR: {:.12}",
            common::units::Units::yocto_f64_to_near_f64(
                bd_cost_value.to_string().parse::<f64>().unwrap_or(0.0)
            )
        );

        println!("\n   💰 Cost Impact Analysis:");
        let old_cost_pct = (old_cost * token_price_near / trade_value) * 100.0;
        let new_cost_pct = (new_cost_value / trade_value) * 100.0;
        let yocto_cost_near = common::units::Units::yocto_f64_to_near_f64(yocto_cost_value);
        let yocto_cost_pct = (yocto_cost_near / trade_value) * 100.0;

        println!("   Old method cost percentage: {:.6}%", old_cost_pct);
        println!("   New method cost percentage: {:.6}%", new_cost_pct);
        println!(
            "   yoctoNEAR method cost percentage: {:.6}%",
            yocto_cost_pct
        );

        let cost_reduction = (old_cost * token_price_near) / new_cost_value;
        println!("   Cost reduction factor: {:.2e}x", cost_reduction);

        // 新しい方法が合理的な範囲内（取引価値の数パーセント）であることを確認
        assert!(
            new_cost_pct > 0.0 && new_cost_pct < 10.0,
            "New cost method should be 0-10% of trade value, got {:.2}%",
            new_cost_pct
        );

        // 修正前は桁違いに大きかったが、今は同程度になっていることを確認
        // (実際の問題はTradingCostの記録方法にあった)
        println!("   注意: 実際の問題はTradingCostの記録部分にありました");

        println!("✅ Cost fix test passed - new method produces reasonable costs");
    }

    #[test]
    fn test_cost_fix_with_different_scenarios() {
        println!("🧪 Testing cost fix with different price scenarios:");

        let scenarios = vec![
            ("Very small price (nearai.aidols.near)", 1.67e-19, 3.00e21),
            ("Small price (akaia.tkn.near)", 3.33e-14, 1.50e16),
            ("Medium price", 1e-6, 1e9),
            ("Large price", 1e-3, 1e6),
        ];

        for (scenario_name, price_near, amount) in scenarios {
            let trade_value = amount * price_near;

            let old_cost_near =
                calculate_trading_cost(amount, &FeeModel::Realistic, 0.01, 0.01) * price_near;

            let new_cost_near =
                calculate_trading_cost_by_value(trade_value, &FeeModel::Realistic, 0.01, 0.01);

            let old_cost_pct = (old_cost_near / trade_value) * 100.0;
            let new_cost_pct = (new_cost_near / trade_value) * 100.0;

            println!("\n   📋 Scenario: {}", scenario_name);
            println!(
                "     Price: {:.2e} NEAR, Amount: {:.2e} tokens",
                price_near, amount
            );
            println!("     Trade value: {:.6} NEAR", trade_value);
            println!(
                "     Old cost: {:.2e} NEAR ({:.2}%)",
                old_cost_near, old_cost_pct
            );
            println!(
                "     New cost: {:.6} NEAR ({:.2}%)",
                new_cost_near, new_cost_pct
            );

            // 新しい方法は常に合理的な範囲内であることを確認
            assert!(
                new_cost_pct > 0.0 && new_cost_pct < 5.0,
                "New cost should be reasonable for {}: {:.2}%",
                scenario_name,
                new_cost_pct
            );
        }

        println!("\n✅ All scenarios produce reasonable costs with new method");
    }
}
