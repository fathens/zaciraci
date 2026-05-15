//! Adapter that builds an `AggregateCapStrategy` from typed-config flags.
//!
//! Pure-side modules (`common::algorithm::vol_targeting`, `regime`,
//! `half_kelly`, `stop_loss`) carry the math; this module reads the flags
//! and assembles the inputs they need. The adapter intentionally stays
//! synchronous and stateless — the only "I/O" is reading config, which is
//! already an in-memory lookup.
//!
//! Stop-loss entry prices are not yet wired through the trade flow; until
//! the per-token entry-price snapshot lands, the stop_loss field is built
//! with an empty entry-price map and the optimizer falls back to no-op for
//! the trigger check (every position is treated as having no entry to
//! compare against).

use common::algorithm::aggregate_cap::AggregateCapStrategy;
use common::algorithm::regime::ExposureScales;
use common::config::ConfigAccess;
use logging::*;
use std::collections::BTreeMap;

/// Build the per-cycle aggregate-cap strategy from the operator-controlled
/// flags. Returns `AggregateCapStrategy::legacy()` when every flag is OFF,
/// which makes the optimizer behave identically to the pre-PR-A path.
pub(crate) fn build_aggregate_cap_strategy(cfg: &impl ConfigAccess) -> AggregateCapStrategy {
    let log = DEFAULT.new(o!("function" => "build_aggregate_cap_strategy"));

    let mut strategy = AggregateCapStrategy::legacy();

    if cfg.portfolio_volatility_target_enabled() {
        let sigma = cfg.portfolio_volatility_target();
        strategy.vol_target_sigma = Some(sigma);
        debug!(log, "vol-targeting enabled"; "sigma_target" => sigma);
    }

    if cfg.portfolio_regime_breadth_enabled() {
        let sma_period = cfg.portfolio_regime_sma_period() as usize;
        let scales = match ExposureScales::new(
            cfg.portfolio_regime_bull_exposure(),
            cfg.portfolio_regime_neutral_exposure(),
            cfg.portfolio_regime_bear_exposure(),
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!(log, "regime exposure scales rejected, falling back to default"; "error" => %e);
                ExposureScales::default()
            }
        };
        strategy.regime_breadth = Some((sma_period, scales));
        debug!(log, "regime-breadth enabled"; "sma_period" => sma_period);
    }

    if cfg.portfolio_half_kelly_enabled() {
        let fraction = cfg.portfolio_half_kelly_fraction();
        strategy.half_kelly_fraction = Some(fraction);
        debug!(log, "half-Kelly enabled"; "fraction" => fraction);
    }

    if cfg.portfolio_stop_loss_enabled() {
        let threshold = cfg.portfolio_stop_loss_threshold();
        // Entry prices are not yet plumbed through the trade flow; until that
        // snapshot lands, the optimizer's stop-loss post-process simply finds
        // no entries and skips every position. The flag still flips on so that
        // simulate sweeps can A/B the wiring overhead.
        strategy.stop_loss = Some((threshold, BTreeMap::new()));
        debug!(log, "stop-loss enabled (entry prices not yet wired)";
            "threshold" => threshold);
    }

    strategy
}
