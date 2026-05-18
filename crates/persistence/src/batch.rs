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

use std::num::NonZeroUsize;

/// Conservative budget for bind parameters per chunk. Leaves headroom below
/// the hard 65535 limit for protocol overhead and future schema growth.
const MAX_BIND_PARAMS_PER_CHUNK: usize = 60_000;

/// Maximum rows that fit in one INSERT statement, given the per-row
/// bind-parameter count.
pub(crate) const fn chunk_rows(cols: NonZeroUsize) -> usize {
    MAX_BIND_PARAMS_PER_CHUNK / cols.get()
}

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
            let rows = chunk_rows(n);
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
        assert_eq!(rows, MAX_BIND_PARAMS_PER_CHUNK);
    }
}
