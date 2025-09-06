pub mod algorithms;
pub mod core;
pub mod data;
pub mod metrics;
pub mod trading;
pub mod types;
pub mod utils;

// Re-export main functions for backward compatibility
pub use algorithms::{
    run_momentum_simulation, run_portfolio_simulation, run_trend_following_simulation,
};
pub use core::{run, validate_and_convert_args};

// Re-export all types for backward compatibility
pub use types::*;

// Re-export utilities for backward compatibility
pub use utils::*;
