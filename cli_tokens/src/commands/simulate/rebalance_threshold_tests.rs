#[cfg(test)]
mod tests {

    #[test]
    fn test_rebalance_thresholds_analysis() {
        println!("🔍 Rebalance Threshold Analysis");

        // 実際のシナリオ: 1000 NEAR資本で0.001 NEAR per tokenの価格
        let initial_capital = 1000.0; // NEAR
        let token_price_near = 0.001; // NEAR per token
        let _tokens_per_near = 1.0 / token_price_near; // 1000 tokens per NEAR

        // 初期保有量: 均等分散で1トークンに全額投資
        let initial_token_amount = initial_capital / token_price_near; // 1,000,000 tokens

        println!("📊 Initial Setup:");
        println!("  - Initial capital: {} NEAR", initial_capital);
        println!("  - Token price: {} NEAR per token", token_price_near);
        println!("  - Initial token amount: {} tokens", initial_token_amount);

        // Portfolio optimization で新しい重みが計算されたとする
        // 例: 現在100%から90%に変更（10%の重み変化）
        let current_weight: f64 = 1.0; // 100%
        let target_weight: f64 = 0.9; // 90%
        let weight_diff = (current_weight - target_weight).abs(); // 0.1 = 10%

        // Portfolio内のREBALANCE_THRESHOLD (5%) との比較
        let portfolio_rebalance_threshold = 0.05; // 5%
        let should_rebalance_by_portfolio = weight_diff > portfolio_rebalance_threshold;

        // 現在のポートフォリオ価値（価格変動なしと仮定）
        let current_portfolio_value = initial_token_amount * token_price_near; // 1000 NEAR

        // 目標トークン量の計算
        let target_token_amount = (current_portfolio_value * target_weight) / token_price_near;
        let token_diff: f64 = (initial_token_amount - target_token_amount).abs(); // tokens

        // Simulation内の相対的閾値 (1% of holdings) との比較
        let relative_threshold = initial_token_amount * 0.01; // 1% of current holdings
        let min_threshold = 0.001; // 最小絶対閾値
        let simulation_rebalance_threshold = relative_threshold.max(min_threshold);
        let should_rebalance_by_simulation = token_diff > simulation_rebalance_threshold;

        println!("\n📈 Weight Change Analysis:");
        println!("  - Current weight: {:.1}%", current_weight * 100.0);
        println!("  - Target weight: {:.1}%", target_weight * 100.0);
        println!("  - Weight difference: {:.1}%", weight_diff * 100.0);
        println!(
            "  - Portfolio threshold: {:.1}%",
            portfolio_rebalance_threshold * 100.0
        );
        println!(
            "  - Should rebalance (Portfolio): {}",
            should_rebalance_by_portfolio
        );

        println!("\n🔄 Token Amount Analysis:");
        println!(
            "  - Current token amount: {:.2} tokens",
            initial_token_amount
        );
        println!("  - Target token amount: {:.2} tokens", target_token_amount);
        println!("  - Token difference: {:.2} tokens", token_diff);
        println!(
            "  - Simulation threshold: {:.2} tokens (1% of holdings)",
            simulation_rebalance_threshold
        );
        println!(
            "  - Should rebalance (Simulation): {}",
            should_rebalance_by_simulation
        );

        // 問題の分析
        if should_rebalance_by_portfolio != should_rebalance_by_simulation {
            println!("\n⚠️  MISMATCH DETECTED!");
            if should_rebalance_by_portfolio && !should_rebalance_by_simulation {
                println!(
                    "  - Portfolio algorithm wants to rebalance, but simulation threshold too high"
                );
                println!(
                    "  - Simulation threshold of {} tokens = {:.6}% of total",
                    simulation_rebalance_threshold,
                    (simulation_rebalance_threshold / initial_token_amount) * 100.0
                );
            } else {
                println!(
                    "  - Simulation would rebalance, but portfolio algorithm threshold too high"
                );
            }
        } else {
            println!("\n✅ Thresholds are consistent");
        }

        println!("\n💡 Analysis Results:");
        println!(
            "  - Simulation threshold: {:.2} tokens ({:.3}% of holdings)",
            simulation_rebalance_threshold,
            (simulation_rebalance_threshold / initial_token_amount) * 100.0
        );

        if simulation_rebalance_threshold < initial_token_amount * 0.005 {
            println!("  - ❌ Current threshold is too small (< 0.5% of holdings)");
        } else if simulation_rebalance_threshold > initial_token_amount * 0.05 {
            println!("  - ❌ Current threshold is too large (> 5% of holdings)");
        } else {
            println!("  - ✅ Current threshold is reasonable (0.5-5% of holdings)");
        }
    }

    #[test]
    fn test_small_vs_large_portfolios() {
        println!("\n🔍 Portfolio Size Impact on Thresholds");

        let simulation_threshold = 0.01; // tokens
        let token_price = 0.001; // NEAR per token

        let portfolio_sizes = vec![10.0, 100.0, 1000.0, 10000.0]; // NEAR

        for &portfolio_size in &portfolio_sizes {
            let token_amount = portfolio_size / token_price;
            let threshold_percentage = (simulation_threshold / token_amount) * 100.0;

            println!(
                "Portfolio size: {} NEAR ({:.0} tokens)",
                portfolio_size, token_amount
            );
            println!(
                "  - 0.01 token threshold = {:.4}% of holdings",
                threshold_percentage
            );

            if threshold_percentage > 1.0 {
                println!("  - ❌ Threshold too high for small portfolios");
            } else if threshold_percentage < 0.001 {
                println!("  - ❌ Threshold too low for large portfolios");
            } else {
                println!("  - ✅ Reasonable threshold");
            }
        }
    }
}
