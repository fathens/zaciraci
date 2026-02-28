pub mod store;
mod typed;

// Re-export everything from store for backward compatibility
pub use store::*;

// Re-export typed config access
pub use typed::{ConfigAccess, ConfigResolver, MockConfig, typed};

#[cfg(test)]
mod tests;
