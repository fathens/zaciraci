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
