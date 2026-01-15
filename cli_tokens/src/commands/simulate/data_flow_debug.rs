#[cfg(test)]
#[allow(clippy::module_inception)]
mod data_flow_debug {
    use super::super::data::get_prices_at_time;
    use bigdecimal::BigDecimal;
    use chrono::{DateTime, Utc};
    use common::stats::ValueAtTime;
    use serde_json;
    use std::collections::HashMap;

    #[test]
    fn test_real_price_data_flow() {
        // 実際のJSONファイルの構造をシミュレート
        let json_content = r#"{
  "metadata": {
    "generated_at": "2025-09-15T04:54:12.449563Z",
    "start_date": "2025-07-02",
    "end_date": "2025-08-11",
    "base_token": "nearai.aidols.near",
    "quote_token": "wrap.near"
  },
  "price_history": {
    "values": [
      {
        "value": 166759.9203717577,
        "time": "2025-07-02T00:07:23.430983"
      }
    ]
  }
}"#;

        #[derive(serde::Deserialize)]
        struct HistoryFile {
            price_history: PriceHistory,
        }

        #[derive(serde::Deserialize)]
        struct PriceHistory {
            values: Vec<ValueAtTime>,
        }

        // JSONをパース
        let history_file: HistoryFile = serde_json::from_str(json_content).unwrap();
        let values = history_file.price_history.values;

        println!("🔍 Real Data Flow Debug:");
        println!("   Raw JSON value: {}", values[0].value.to_f64());
        println!("   Raw JSON time: {:?}", values[0].time);

        // NEAR単位への変換をテスト
        let yocto_value = values.clone()[0].value.clone().into_bigdecimal();
        let near_value = common::units::Units::yocto_to_near(&yocto_value);
        println!("   Converted to NEAR: {:.2e} NEAR", near_value);

        // get_prices_at_timeの動作をテスト
        let mut price_data = HashMap::new();
        price_data.insert("nearai.aidols.near".to_string(), values);

        let target_time = DateTime::parse_from_rfc3339("2025-07-02T00:07:23.430983Z")
            .unwrap()
            .with_timezone(&Utc);

        let prices = get_prices_at_time(&price_data, target_time).unwrap();
        let returned_price = prices.get("nearai.aidols.near").unwrap();

        println!("   get_prices_at_time returned: {}", returned_price);
        println!("   Expected yoctoNEAR value: {}", yocto_value);

        // 値が同じことを確認（f64からBigDecimalへの変換のため精度の問題があるかもしれない）
        let returned_as_bigdecimal = returned_price.to_string().parse::<BigDecimal>().unwrap();
        let diff = (&returned_as_bigdecimal - &yocto_value).abs();
        assert!(
            diff < "0.01".parse::<BigDecimal>().unwrap(),
            "Values should be approximately equal"
        );

        // 問題：この値を使った時の初期ポートフォリオ計算
        let initial_capital = BigDecimal::from(1000); // NEAR
        let initial_per_token = &initial_capital / BigDecimal::from(2); // 2つのトークンを想定
        let initial_price_near =
            common::units::Units::yocto_f64_to_near_f64(returned_price.as_f64())
                .to_string()
                .parse::<BigDecimal>()
                .unwrap();
        let token_amount = &initial_per_token / &initial_price_near;

        println!("   💰 Portfolio Calculation:");
        println!("   initial_capital: {} NEAR", initial_capital);
        println!("   initial_per_token: {} NEAR", initial_per_token);
        println!("   price_yocto: {}", returned_price);
        println!("   price_near: {:.2e} NEAR", initial_price_near);
        println!("   calculated_token_amount: {:.2e}", token_amount);

        // これが問題の原因！
        if token_amount > "1e20".parse::<BigDecimal>().unwrap() {
            println!("   ❌ PROBLEM: Token amount is astronomical!");
            println!("   This explains the 5.45e20 trade amounts in momentum algorithm");
        }

        // 実際の取引で何が起きるかシミュレート
        let simulated_current_holdings =
            HashMap::from([("nearai.aidols.near".to_string(), token_amount)]);

        println!("   📊 Simulated Holdings:");
        for (token, amount) in &simulated_current_holdings {
            println!("   {}: {:.2e} tokens", token, amount);

            // ポートフォリオ価値の計算
            let portfolio_value = amount * initial_price_near.clone();
            println!("   Portfolio value: {:.6} NEAR", portfolio_value);
        }

        // TradeContextで使われる値をシミュレート
        if let Some(current_amount) = simulated_current_holdings.get("nearai.aidols.near") {
            println!("   🔄 Trading Context Simulation:");
            println!(
                "   current_amount (for TradeContext): {:.2e}",
                current_amount
            );

            if current_amount > &"1e20".parse::<BigDecimal>().unwrap() {
                println!("   ❌ This current_amount would cause astronomical trades!");
                println!("   In TradeExecution.amount, this becomes the 5.45e20 we saw");
            }
        }
    }

    #[test]
    fn test_price_filtering_solution() {
        // 極端に小さい価格のトークンフィルタリングのテスト
        println!("🛡️ Price Filtering Solution Test:");

        let price_threshold = 1e-15; // NEAR
        let test_prices = vec![
            ("good_token", 1e-12),       // OK: 1e-12 NEAR > 1e-15
            ("borderline_token", 1e-15), // Borderline: exactly at threshold
            ("bad_token", 1.67e-19),     // Skip: too small
        ];

        println!("   Price threshold: {:.2e} NEAR", price_threshold);

        for (token, price_near) in test_prices {
            let should_skip = price_near < price_threshold;
            let status = if should_skip { "❌ SKIP" } else { "✅ TRADE" };

            println!("   {}: {:.2e} NEAR -> {}", token, price_near, status);

            if !should_skip {
                // トークン数量を計算してみる
                let capital = 500.0; // NEAR
                let token_amount = capital / price_near;
                println!("      Token amount: {:.2e}", token_amount);

                // 現実的な範囲内であることを確認
                assert!(token_amount < 1e20, "Token amount should be reasonable");
            }
        }
    }

    #[test]
    fn test_expected_vs_actual_calculation() {
        // 期待値：適切な計算
        println!("🧮 Expected vs Actual Calculation Comparison:");

        let price_yocto = 166759.9203717577;
        let price_near = common::units::Units::yocto_f64_to_near_f64(price_yocto);

        println!(
            "   Price: {} yoctoNEAR = {:.2e} NEAR",
            price_yocto, price_near
        );

        // 現在の計算方式
        let capital_near = 500.0; // 1000 NEAR / 2 tokens
        let current_token_amount = capital_near / price_near;

        println!("   Current Calculation:");
        println!("   capital_per_token: {} NEAR", capital_near);
        println!("   token_amount: {:.2e}", current_token_amount);

        // 問題の確認
        if current_token_amount > 1e20 {
            println!("   ❌ Current calculation produces astronomical token amounts");
        }

        // 代替案1: 最小単位での計算
        let capital_yocto = common::units::Units::near_f64_to_yocto_f64(capital_near);
        let token_amount_yocto_based = capital_yocto / price_yocto;

        println!("   Alternative (yocto-based):");
        println!("   capital_yocto: {:.2e}", capital_yocto);
        println!("   token_amount: {:.2e}", token_amount_yocto_based);

        // 精度の比較
        println!("   Precision comparison:");
        println!("   current_method: {:.6e}", current_token_amount);
        println!("   yocto_method: {:.6e}", token_amount_yocto_based);
        println!(
            "   difference: {:.6e}",
            (current_token_amount - token_amount_yocto_based).abs()
        );

        // 両方の結果で価値を逆算
        let value_current = current_token_amount * price_near;
        let value_yocto = token_amount_yocto_based * price_near;

        println!("   Value verification:");
        println!("   current_method value: {:.6} NEAR", value_current);
        println!("   yocto_method value: {:.6} NEAR", value_yocto);
        println!("   expected value: {} NEAR", capital_near);
    }
}
