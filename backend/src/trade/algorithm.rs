// Use algorithm implementations from common crate
pub use zaciraci_common::algorithm::*;

use crate::Result;

// Types are defined in common/algorithm/types.rs and re-exported above

// Functions are defined in common/algorithm/indicators.rs and re-exported above
// Use the common crate functions: calculate_sharpe_ratio, calculate_max_drawdown, etc.

// ==================== トレイト定義 ====================

/// 取引アルゴリズムの共通インターフェース
pub trait TradingAlgorithm {
    type Config;
    type Signal;

    /// アルゴリズムの初期化
    fn new(config: Self::Config) -> Self;

    /// 市場データから取引シグナルを生成
    fn generate_signal(&self, market_data: &MarketData) -> Result<Option<Self::Signal>>;

    /// アルゴリズムの名前を取得
    fn name(&self) -> &str;

    /// パフォーマンス指標を計算
    fn calculate_performance(&self, trades: &[TradeExecution]) -> Result<PerformanceMetrics>;
}

// Use PerformanceMetrics from common crate

// ==================== テスト ====================

#[cfg(test)]
mod tests {
    use super::*;

    // Tests moved to common crate tests
    // Use zaciraci-common for algorithm testing

    #[test]
    fn test_algorithm_import() {
        // Test that common crate types are properly imported
        let values = vec![100.0, 110.0, 90.0, 120.0, 80.0, 150.0];
        let max_dd = calculate_max_drawdown(&values);
        assert!(max_dd > 0.0);

        let returns = vec![0.1, -0.05, 0.2, 0.0, 0.15];
        let sharpe = calculate_sharpe_ratio(&returns, 0.02);
        assert!(sharpe.is_finite());
    }
}
