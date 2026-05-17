//! Bind-parameter budgeting for Diesel batch inserts.
//!
//! PostgreSQL allows at most 65535 bind parameters per query (the wire
//! protocol uses an i16 for the parameter count). A naive
//! `INSERT … VALUES (...), (...), ...` produces `rows × cols` parameters;
//! for tables whose row count grows with on-chain state (e.g. REF Finance
//! pools), the single-statement batch insert eventually exceeds this limit
//! and starts failing with `number of parameters must be between 0 and 65535`,
//! silently halting DB writes until the input shrinks again.

/// Conservative budget for bind parameters per chunk. Leaves headroom below
/// the hard 65535 limit for protocol overhead and future schema growth.
const MAX_BIND_PARAMS_PER_CHUNK: usize = 60_000;

/// Maximum rows that fit in one INSERT statement, given the per-row
/// bind-parameter count. `cols` must be > 0.
pub(crate) const fn chunk_rows(cols: usize) -> usize {
    assert!(cols > 0, "cols must be > 0");
    MAX_BIND_PARAMS_PER_CHUNK / cols
}
