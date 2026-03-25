# Simulation Accuracy Verification Report

**Date**: 2026-03-25
**Analysis Period**: 2025-10-01 to 2026-03-25
**Data Source**: run_local (production DB, readonly)

## Summary

The simulation's core swap calculation (`estimate_return()`) is highly accurate.
76% of trades with actual output data show **zero divergence** from blockchain execution results.

## Key Metrics

| Metric | Value |
|--------|-------|
| Total trades | 535 |
| Trades with actual output | 34 (6.4%) |
| Trades without actual output | 501 (93.6%) |
| Mean error | +0.43% |
| Median error | 0.00% |
| Std dev | 2.26% |
| 95th percentile | 0.68% |
| Max error | 13.38% |

**Note**: `actual_to_amount` recording was added in March 2026. Earlier trades lack actual output data.

## Error Direction

- Positive error = actual > estimated (favorable execution, estimate is conservative)
- Negative error = actual < estimated (slippage loss)
- Overall bias: slightly conservative (mean +0.43%)

## Per Token Pair Breakdown

| Pair | Avg Error | Max Error | Count |
|------|-----------|-----------|-------|
| wrap.near -> apys.token.a11bd.near | 0.00% | 0.00% | 4 |
| wrap.near -> itlx.intellex_xyz.near | 0.00% | 0.00% | 4 |
| wrap.near -> rin.tkn.near | 0.00% | 0.00% | 3 |
| wrap.near -> blackdragon.tkn.near | -0.0001% | 0.0004% | 4 |
| wrap.near -> score.aidols.near | +0.06% | 0.28% | 5 |
| wrap.near -> ftv2.nekotoken.near | +0.16% | 0.68% | 6 |
| wrap.near -> nearkat.tkn.near | +3.34% | **13.38%** | 4 |
| ftv2.nekotoken.near -> wrap.near | 0.00% | 0.00% | 2 |
| nearkat.tkn.near -> wrap.near | 0.00% | 0.00% | 1 |
| rin.tkn.near -> wrap.near | 0.00% | 0.00% | 1 |

## Outlier Analysis

The single large outlier (nearkat.tkn.near, +13.38%) is likely caused by pool state
changes between `estimate_return()` calculation and blockchain execution. This token's
pool may have lower liquidity or higher trading activity from other participants.

Excluding this outlier, the mean error drops to approximately +0.03%.

## Identified Divergence Sources

### 1. AMM Calculation Accuracy (LOW impact)
`estimate_return()` matches blockchain output exactly in 76% of cases.
The formula correctly models fees and price impact.

### 2. Pool State Timing (LOW-MEDIUM impact)
- Simulation: reads pool at `sim_day` start
- Real trade: reads latest pool at execution time
- Impact: usually negligible except for volatile/low-liquidity pools

### 3. Gas Fees (LOW impact)
- Not modeled in simulation
- Estimated total: ~5.35 NEAR across 535 trades (~0.01 NEAR/trade)
- As % of typical 100 NEAR capital: 5.35%

### 4. DbRate Fallback (HIGH impact - confirmed)
- Simulation falls back to fee-less DB rate conversion when pool data unavailable
- **Confirmed**: Existing simulation run (2026-03-15 to 2026-03-25) shows **100% fallback rate** (19/19 swaps)
- Root cause: `pool_info` table retention is 10 snapshots per pool, and production only
  stores the latest snapshots (all from 2026-03-25). Historical pool data is not retained.
- `read_from_db(Some(sim_day))` queries `timestamp < sim_day`, finding no data for past dates
- **This means all simulation swaps use fee-less rate conversion**, systematically
  overestimating returns by the pool fee amount (typically 0.30%)

### 5. Transaction Failures (NOT measurable)
- Failed transactions are not recorded in trade_transactions
- Simulation always succeeds

## Conclusion

The `estimate_return()` AMM formula is highly accurate (76% exact match with blockchain).
However, the simulation currently **cannot use pool-based calculation at all** because
historical pool data is not retained in production. All swaps fall back to DbRate
(fee-less rate conversion), which systematically overestimates returns.

### Recommendations

1. **HIGH priority**: Fix the DbRate fallback problem. Options:
   - (a) Increase `pool_info_retention_count` to retain historical snapshots for simulation
   - (b) Change simulation to use the nearest available pool snapshot instead of requiring
     `timestamp < sim_day`
   - (c) Pre-populate pool data for simulation period before running
2. **LOW priority**: Gas fee model (cumulative ~5.35% on 100 NEAR over 535 trades)
3. **Monitor**: nearkat-type outliers as more actual_to_amount data accumulates
