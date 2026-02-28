pub mod startup;
pub mod store;
mod typed;

pub use typed::{ConfigAccess, ConfigResolver, MockConfig, typed};

#[cfg(test)]
mod tests;
