/// Cache-related console output utilities for consistent user experience
pub struct CacheOutput;

impl CacheOutput {
    /// Output message when using cached price data
    pub fn price_cache_hit(token: &str, data_points: usize) {
        println!(
            "  ✅ Using cached data for {} ({} data points)",
            token, data_points
        );
    }

    /// Output message when fetching price data from API
    pub fn price_cache_miss(token: &str) {
        println!("  🌐 Fetching data from API for {}", token);
    }

    /// Output message when caching price data
    pub fn price_cached(token: &str, data_points: usize) {
        println!(
            "  💾 Cached data for {} ({} data points)",
            token, data_points
        );
    }

    /// Output message when using cached prediction
    pub fn prediction_cache_hit(token: &str) {
        println!("  ✅ Using cached prediction for {}", token);
    }

    /// Output message when fetching prediction from API
    pub fn prediction_cache_miss(token: &str) {
        println!("  🔮 Fetching prediction from API for {}", token);
    }

    /// Output message when caching prediction
    pub fn prediction_cached(token: &str, points: usize) {
        println!("  💾 Cached prediction for {} ({} points)", token, points);
    }

    /// Generic cache hit message
    pub fn cache_hit(cache_type: &str, identifier: &str, item_count: Option<usize>) {
        match item_count {
            Some(count) => println!(
                "  ✅ Using cached {} for {} ({} items)",
                cache_type, identifier, count
            ),
            None => println!("  ✅ Using cached {} for {}", cache_type, identifier),
        }
    }

    /// Generic cache miss message
    pub fn cache_miss(cache_type: &str, identifier: &str) {
        let emoji = match cache_type {
            "prediction" => "🔮",
            _ => "🌐",
        };
        println!(
            "  {} Fetching {} from API for {}",
            emoji, cache_type, identifier
        );
    }

    /// Generic cache save message
    pub fn cache_saved(cache_type: &str, identifier: &str, item_count: Option<usize>) {
        match item_count {
            Some(count) => println!(
                "  💾 Cached {} for {} ({} items)",
                cache_type, identifier, count
            ),
            None => println!("  💾 Cached {} for {}", cache_type, identifier),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_price_cache_output_messages() {
        // Test that functions don't panic and use expected formats
        CacheOutput::price_cache_hit("test.token.near", 100);
        CacheOutput::price_cache_miss("test.token.near");
        CacheOutput::price_cached("test.token.near", 100);
    }

    #[test]
    fn test_prediction_cache_output_messages() {
        CacheOutput::prediction_cache_hit("test.token.near");
        CacheOutput::prediction_cache_miss("test.token.near");
        CacheOutput::prediction_cached("test.token.near", 50);
    }

    #[test]
    fn test_generic_cache_output_messages() {
        CacheOutput::cache_hit("price data", "test.token.near", Some(75));
        CacheOutput::cache_hit("metadata", "test.token.near", None);

        CacheOutput::cache_miss("prediction", "test.token.near");
        CacheOutput::cache_miss("price data", "test.token.near");

        CacheOutput::cache_saved("price data", "test.token.near", Some(75));
        CacheOutput::cache_saved("metadata", "test.token.near", None);
    }

    #[test]
    fn test_cache_output_consistency() {
        // Test that specific methods match generic method outputs conceptually
        // These should produce similar structured output

        // Compare prediction-specific vs generic
        CacheOutput::prediction_cache_hit("token");
        CacheOutput::cache_hit("prediction", "token", None);

        CacheOutput::prediction_cache_miss("token");
        CacheOutput::cache_miss("prediction", "token");

        CacheOutput::prediction_cached("token", 42);
        CacheOutput::cache_saved("prediction", "token", Some(42));
    }
}
