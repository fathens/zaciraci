//! `ref_flow_collector_state`: the collector's single-row cursor state,
//! guarded by a compare-and-swap version.
//!
//! The payload is opaque JSON here; the typed `CollectorState` lives in
//! the `ref_flow` crate. A save must present the version it loaded, so a
//! second writer (overlapping deploy, importer) is detected as a
//! [`SaveOutcome::Conflict`] instead of silently interleaving writes.

use crate::Result;
use crate::connection_pool;
use crate::schema::ref_flow_collector_state;
use anyhow::{Context, anyhow};
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use serde::de::DeserializeOwned;

const ROW_ID: i16 = 1;

/// A loaded state row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    /// Present this value to [`save`]; it changes on every successful save.
    pub version: u64,
    /// Diagnostic: who wrote the row last.
    pub writer_id: String,
    pub state: T,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = ref_flow_collector_state)]
#[diesel(check_for_backend(diesel::pg::Pg))]
struct DbState {
    version: i64,
    writer_id: String,
    state: serde_json::Value,
    updated_at: NaiveDateTime,
}

/// Load the state row, if initialized. A payload that no longer matches
/// `T` is an error, never a silent reset.
pub async fn load<T: DeserializeOwned>() -> Result<Option<Versioned<T>>> {
    let conn = connection_pool::get().await?;
    let row = conn
        .interact(|conn| {
            ref_flow_collector_state::table
                .filter(ref_flow_collector_state::id.eq(ROW_ID))
                .select(DbState::as_select())
                .first::<DbState>(conn)
                .optional()
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    row.map(|row| {
        let state: T = serde_json::from_value(row.state)
            .context("ref_flow_collector_state.state does not match the expected schema")?;
        Ok(Versioned {
            version: u64::try_from(row.version).context("negative state version")?,
            writer_id: row.writer_id,
            state,
            updated_at: row.updated_at,
        })
    })
    .transpose()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitOutcome {
    /// The row was created with version 1.
    Created,
    /// A row already existed; nothing was written.
    AlreadyInitialized,
}

/// Create the state row (version 1) if it does not exist yet.
pub async fn initialize<T: Serialize>(
    writer_id: &str,
    state: &T,
    now: NaiveDateTime,
) -> Result<InitOutcome> {
    let payload = serde_json::to_value(state).context("serialize collector state")?;
    let writer_id = writer_id.to_owned();
    let conn = connection_pool::get().await?;
    let inserted = conn
        .interact(move |conn| {
            diesel::insert_into(ref_flow_collector_state::table)
                .values((
                    ref_flow_collector_state::id.eq(ROW_ID),
                    ref_flow_collector_state::version.eq(1i64),
                    ref_flow_collector_state::writer_id.eq(writer_id),
                    ref_flow_collector_state::state.eq(payload),
                    ref_flow_collector_state::updated_at.eq(now),
                ))
                .on_conflict(ref_flow_collector_state::id)
                .do_nothing()
                .execute(conn)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    Ok(if inserted == 1 {
        InitOutcome::Created
    } else {
        InitOutcome::AlreadyInitialized
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Written; the row now carries this version.
    Saved { version: u64 },
    /// The row's version differs from `expected_version`: another writer
    /// saved in between. Nothing was written.
    Conflict,
}

/// Compare-and-swap save: succeeds only if the row still carries
/// `expected_version`. Fails (as an error) when the row was never
/// initialized.
pub async fn save<T: Serialize>(
    expected_version: u64,
    writer_id: &str,
    state: &T,
    now: NaiveDateTime,
) -> Result<SaveOutcome> {
    let payload = serde_json::to_value(state).context("serialize collector state")?;
    let expected = i64::try_from(expected_version).context("version exceeds i64")?;
    let writer_id = writer_id.to_owned();
    let conn = connection_pool::get().await?;
    let outcome = conn
        .interact(move |conn| -> QueryResult<Option<i64>> {
            let updated = diesel::update(
                ref_flow_collector_state::table
                    .filter(ref_flow_collector_state::id.eq(ROW_ID))
                    .filter(ref_flow_collector_state::version.eq(expected)),
            )
            .set((
                ref_flow_collector_state::version.eq(expected + 1),
                ref_flow_collector_state::writer_id.eq(writer_id),
                ref_flow_collector_state::state.eq(payload),
                ref_flow_collector_state::updated_at.eq(now),
            ))
            .returning(ref_flow_collector_state::version)
            .get_result::<i64>(conn)
            .optional()?;
            Ok(updated)
        })
        .await
        .map_err(|e| anyhow!("Database interaction error: {:?}", e))??;
    match outcome {
        Some(version) => Ok(SaveOutcome::Saved {
            version: u64::try_from(version).context("negative state version")?,
        }),
        None => {
            let exists = load::<serde_json::Value>().await?.is_some();
            if exists {
                Ok(SaveOutcome::Conflict)
            } else {
                Err(anyhow!("ref_flow_collector_state is not initialized"))
            }
        }
    }
}

#[cfg(test)]
mod tests;
