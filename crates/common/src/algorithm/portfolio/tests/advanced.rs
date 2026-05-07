// Advanced portfolio tests, split into topical submodules per
// CONTRIBUTING.md (test files exceeding 2,000 lines must be subdivided).
//
// IMPORTANT: `pub use super::*;` must remain `pub` so that submodules can
// access the parent test module's re-exports (BigDecimal, BTreeMap,
// PortfolioData, etc.) via their own `use super::*;`. Removing the `pub`
// breaks the chain and causes build failures.

pub use super::*;

mod algorithm_validation;
mod confidence_alpha;
mod nan_inf_defense;
mod parallel_consistency;
mod precision;
mod returns_from_prices;
mod sharpe_equal_returns;
