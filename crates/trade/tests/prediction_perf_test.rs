//! Prediction performance test using real DB data (run_test environment).
//!
//! Requires: run_test PostgreSQL running on localhost:5433 with production-like data.
//! Run with: DATABASE_URL=postgres://postgres_test:postgres_test@localhost:5433/postgres_test \
//!           cargo test -p trade --test prediction_perf_test -- --nocapture --ignored

use common::config::ConfigResolver;

static CFG: ConfigResolver = ConfigResolver;

/// Run a full prediction cycle against the test database and measure timing.
///
/// This test exercises the complete prediction pipeline:
/// 1. select_prediction_target_tokens (volatility + liquidity filter)
/// 2. get_rates_for_multiple_tokens (batch DB fetch of 30-day histories)
/// 3. predict_multiple_tokens (buffer_unordered with chronos-rs predictions)
/// 4. record_predictions (batch DB insert)
///
/// Monitor memory externally via `docker stats` or similar while this runs.
#[tokio::test]
#[ignore] // Requires run_test DB with data; run manually
async fn test_prediction_cycle_perf() {
    let start = std::time::Instant::now();

    let as_of = chrono::Utc::now();
    match trade::run_prediction_cycle(as_of, &CFG).await {
        Ok(count) => {
            let elapsed = start.elapsed();
            println!("=== Prediction Cycle Complete ===");
            println!("Predictions recorded: {count}");
            println!("Total time: {:.2}s", elapsed.as_secs_f64());
            println!(
                "Avg per token: {:.2}s",
                elapsed.as_secs_f64() / count.max(1) as f64
            );
        }
        Err(e) => {
            let elapsed = start.elapsed();
            println!(
                "=== Prediction Cycle Failed after {:.2}s ===",
                elapsed.as_secs_f64()
            );
            println!("Error: {e:#}");
            // Don't panic - this test is for measurement, not correctness.
            // Failure may be expected if DB has no data.
        }
    }
}
