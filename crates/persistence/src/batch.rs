//! Bind-parameter budgeting for Diesel batch inserts.
//!
//! PostgreSQL allows at most 65535 bind parameters per query (the wire
//! protocol uses an i16 for the parameter count). A naive
//! `INSERT … VALUES (...), (...), ...` produces `rows × cols` parameters;
//! for tables whose row count grows with on-chain state (e.g. REF Finance
//! pools), the single-statement batch insert eventually exceeds this limit
//! and starts failing with `number of parameters must be between 0 and 65535`,
//! silently halting DB writes until the input shrinks again.
//!
//! Each `Insertable` struct must declare its bind-parameter count as
//! `const COLS: NonZeroUsize` co-located with the struct definition, and
//! call sites must derive the chunk size via `chunk_rows(Self::COLS)`.
//! This keeps the SSoT for column counts next to the field list so that
//! adding a field forces an update of `COLS`; the structural tests in
//! each insert module additionally enforce this with an exhaustive
//! destructuring pattern that fails to compile on field-count drift.
//!
//! ## Logging policy
//!
//! The `debug!(log, "batch chunked"; ...)` line emitted by every call site
//! MUST stay limited to integer aggregate counts (`rows`, `chunk_rows`,
//! `chunks`). Do not extend with per-row identifiers — token names,
//! tx_ids, wallet addresses, `BigDecimal` amounts — even at `debug` level.
//! Those values survive log retention windows and propagate to forwarding
//! pipelines that may have weaker access controls than the DB itself.

use std::num::NonZeroUsize;

/// Conservative budget for bind parameters per chunk.
///
/// The PostgreSQL wire protocol caps parameters at 65535 (i16). The 5535
/// headroom below that cap (≈8.4%) absorbs protocol overhead and future
/// schema growth: adding one column to the worst-case row (currently
/// `TradeTransaction`, COLS=9 → CHUNK_ROWS=6666) shrinks the row budget
/// to ≈6000 (≈10% reduction), still well clear of the hard limit.
const MAX_BIND_PARAMS_PER_CHUNK: usize = 60_000;

/// Maximum rows that fit in one INSERT statement, given the per-row
/// bind-parameter count.
///
/// Returns `NonZeroUsize` so call sites cannot accidentally feed `0` into
/// `slice::chunks` (which panics) or `Vec::drain(..0)` (which loops
/// forever). The `const` panic fires at compile time if `cols` is large
/// enough that the integer division underflows to 0 — for the workspace's
/// current `Insertable` set (COLS ∈ {6,7,8,9}) this never triggers, but
/// it guards future schema growth past the budget.
pub(crate) const fn chunk_rows(cols: NonZeroUsize) -> NonZeroUsize {
    match NonZeroUsize::new(MAX_BIND_PARAMS_PER_CHUNK / cols.get()) {
        Some(n) => n,
        None => panic!(
            "MAX_BIND_PARAMS_PER_CHUNK divided by cols underflowed to 0; cols exceeds the budget",
        ),
    }
}

/// Test-only `chunk_rows` variant that takes an explicit `budget` instead of
/// `MAX_BIND_PARAMS_PER_CHUNK`. Tests shrink the budget (e.g. 4 with COLS=2)
/// so a 3-row insert spans two chunks, exercising the chunk-boundary
/// atomicity contract without the cost of inserting 6666+ rows. The same
/// underflow-to-zero guard as `chunk_rows` applies.
#[cfg(test)]
pub(crate) const fn chunk_rows_with_budget(
    budget: NonZeroUsize,
    cols: NonZeroUsize,
) -> NonZeroUsize {
    match NonZeroUsize::new(budget.get() / cols.get()) {
        Some(n) => n,
        None => panic!("budget divided by cols underflowed to 0; budget < cols"),
    }
}

/// Compile-time assertion that `<ty>::COLS` matches the destructuring
/// pattern's field list. Adding or removing an `Insertable` field without
/// updating `COLS` triggers a compile error: the destructuring is
/// exhaustive, and the array literal's length is type-checked against
/// `<ty>::COLS`. Caller supplies the field name list explicitly so
/// auto-discovery does not silently absorb a new field.
#[cfg(test)]
macro_rules! enforce_cols_matches_fields {
    ($ty:ident { $($field:ident),+ $(,)? }) => {
        #[test]
        fn cols_matches_struct_fields() {
            fn _enforce(v: $ty) {
                let $ty { $($field),+ } = v;
                let _: [(); <$ty>::COLS.get()] = [
                    $({ let _ = $field; }),+
                ];
            }
            let _: fn($ty) = _enforce;
        }
    };
}

#[cfg(test)]
pub(crate) use enforce_cols_matches_fields;

#[cfg(test)]
mod tests {
    use super::*;

    /// `chunk_rows` must always produce a row count whose total bind-param
    /// usage stays at or below the configured budget, for every column
    /// count the workspace currently uses.
    #[test]
    fn chunk_rows_stays_within_budget() {
        for cols in [1usize, 6, 7, 8, 9, 100, 1_000, MAX_BIND_PARAMS_PER_CHUNK] {
            let n = NonZeroUsize::new(cols).expect("test input must be non-zero");
            let rows = chunk_rows(n).get();
            assert!(
                rows * cols <= MAX_BIND_PARAMS_PER_CHUNK,
                "chunk_rows({cols}) = {rows} exceeds budget: {} > {}",
                rows * cols,
                MAX_BIND_PARAMS_PER_CHUNK,
            );
        }
    }

    /// `cols = 1` must consume the full budget (no wasted headroom from
    /// the integer division).
    #[test]
    fn chunk_rows_uses_full_budget_at_cols_one() {
        let rows = chunk_rows(NonZeroUsize::new(1).expect("non-zero"));
        assert_eq!(rows.get(), MAX_BIND_PARAMS_PER_CHUNK);
    }

    /// `chunk_rows_with_budget` must yield the test-friendly small chunk
    /// sizes that exercise multi-chunk paths without large row counts.
    #[test]
    fn chunk_rows_with_budget_supports_small_chunks() {
        let budget = NonZeroUsize::new(4).expect("non-zero");
        let cols = NonZeroUsize::new(2).expect("non-zero");
        assert_eq!(chunk_rows_with_budget(budget, cols).get(), 2);
    }
}
