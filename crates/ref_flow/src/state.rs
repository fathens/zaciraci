//! The collector's cursor state, persisted as JSON in
//! `ref_flow_collector_state` behind a compare-and-swap version.

use serde::{Deserialize, Serialize};

/// Bump when the JSON layout changes incompatibly. A stored state with a
/// different version is an error, never silently reset.
pub const SCHEMA_VERSION: u32 = 1;
/// A gap list longer than this means something is badly wrong (every
/// cycle closes gaps far faster than forward can open them).
pub const MAX_GAPS: usize = 64;

/// A hole in the receipt list that still has to be listed: receipts with
/// block time in `(until_ns, oldest_ns]` have not been seen yet (or none,
/// if `oldest_ns` is unknown). Listing resumes from `cursor` when present,
/// otherwise from just after `oldest_ns`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    pub cursor: Option<String>,
    /// Newest receipt time already covered below the gap (exclusive).
    pub until_ns: u64,
    /// Oldest receipt time reached so far inside the gap.
    pub oldest_ns: Option<u64>,
}

impl Gap {
    /// Remaining span in nanoseconds (0 when the gap has no listed edge).
    pub fn span_ns(&self) -> u64 {
        self.oldest_ns
            .map(|o| o.saturating_sub(self.until_ns))
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectorState {
    pub schema_version: u32,
    /// Newest receipt block time (ns) the forward pass has covered.
    pub since_ns: u64,
    pub gaps: Vec<Gap>,
    /// Rate-limit cooldown: no requests before this time (UTC seconds).
    pub cooldown_until: Option<i64>,
    /// Diagnostics.
    pub last_cycle_at: Option<chrono::NaiveDateTime>,
    pub last_success_at: Option<chrono::NaiveDateTime>,
}

impl CollectorState {
    /// Fresh state whose forward cursor starts at `since_ns`.
    pub fn new(since_ns: u64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            since_ns,
            gaps: Vec::new(),
            cooldown_until: None,
            last_cycle_at: None,
            last_success_at: None,
        }
    }

    /// Validate a state loaded from the DB.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            anyhow::bail!(
                "collector state schema {} is not the supported {}",
                self.schema_version,
                SCHEMA_VERSION
            );
        }
        if self.gaps.len() > MAX_GAPS {
            anyhow::bail!(
                "collector state has {} gaps, more than {MAX_GAPS}",
                self.gaps.len()
            );
        }
        Ok(())
    }

    /// Register a gap, refusing to grow past [`MAX_GAPS`].
    pub fn push_gap(&mut self, gap: Gap) -> anyhow::Result<()> {
        if self.gaps.len() >= MAX_GAPS {
            anyhow::bail!("gap list is full ({MAX_GAPS}); refusing to add another");
        }
        self.gaps.push(gap);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
