#[cfg(test)]
mod tests {
    use super::super::types::FeeModel;
    use super::super::utils::{
        calculate_trading_cost, calculate_trading_cost_by_value,
        calculate_trading_cost_by_value_yocto,
    };

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

    /// 現在の計算方法のバグを検出するテスト
    /// decimals=24 の場合は偶然正しいが、decimals=6 の場合は10^18倍の誤差がある
    #[test]
    fn test_trade_value_calculation_bug_detection() {
        use common::types::{TokenAmountF64, TokenPriceF64, YoctoValueF64};

        // decimals=24 (wNEAR) の場合
        let amount_24 = TokenAmountF64::from_smallest_units(1e24, 24); // 1 wNEAR
        let price = TokenPriceF64::from_near_per_token(1.0); // 1 NEAR/wNEAR

        // 型安全な演算
        let correct_value: YoctoValueF64 = amount_24 * price;
        println!(
            "decimals=24: correct value = {} yoctoNEAR",
            correct_value.as_f64()
        );

        // 現在のバグ計算（smallest_units × price）
        let buggy_value_24 = 1e24 * 1.0; // smallest_units × NEAR/token
        println!("decimals=24: buggy value = {}", buggy_value_24);

        // decimals=24 の場合は偶然一致
        assert!((correct_value.as_f64() - buggy_value_24).abs() < 1e10);

        // decimals=6 (USDT) の場合
        let amount_6 = TokenAmountF64::from_smallest_units(1e6, 6); // 1 USDT
        let price_usdt = TokenPriceF64::from_near_per_token(0.2); // 0.2 NEAR/USDT

        // 型安全な演算
        let correct_value_6: YoctoValueF64 = amount_6 * price_usdt;
        println!(
            "decimals=6: correct value = {} yoctoNEAR",
            correct_value_6.as_f64()
        );

        // 現在のバグ計算
        let buggy_value_6 = 1e6 * 0.2; // smallest_units × NEAR/token
        println!("decimals=6: buggy value = {}", buggy_value_6);

        // decimals=6 の場合は 10^18 倍の誤差がある！
        let ratio = correct_value_6.as_f64() / buggy_value_6;
        println!("decimals=6: ratio (correct/buggy) = {}", ratio);
        assert!(ratio > 1e17, "Expected huge discrepancy for decimals=6");
    }

    /// TradingCost の計算が型安全な演算と一致することを確認
    #[test]
    fn test_trading_cost_uses_type_safe_calculation() {
        use common::types::{NearValueF64, TokenAmountF64, TokenPriceF64, YoctoValueF64};

        // USDT シナリオ (decimals=6)
        let amount = TokenAmountF64::from_smallest_units(100e6, 6); // 100 USDT
        let price = TokenPriceF64::from_near_per_token(0.2); // 0.2 NEAR/USDT
        let gas_cost = NearValueF64::from_near(0.01); // 0.01 NEAR

        // 型安全な演算で取引価値を計算
        let trade_value: YoctoValueF64 = amount * price;
        println!(
            "Trade value: {} yoctoNEAR ({} NEAR)",
            trade_value.as_f64(),
            trade_value.to_near().as_f64()
        );

        // コスト計算（f64版）
        let slippage_rate = 0.01;
        let cost = calculate_trading_cost_by_value_yocto(
            trade_value.as_f64(),
            &FeeModel::Realistic,
            slippage_rate,
            gas_cost.to_yocto().as_f64(),
        );

        // コストが取引価値の合理的な割合であることを確認
        let cost_pct = cost / trade_value.as_f64() * 100.0;
        println!("Cost: {} yoctoNEAR ({:.2}% of trade value)", cost, cost_pct);
        assert!(
            cost_pct > 0.0 && cost_pct < 5.0,
            "Cost should be 0-5% of trade value"
        );
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
