-- Runtime state of the ref_flow collector (crates/ref_flow), replacing the
-- state.json / ledger.json / runs.csv files of the Python collector.

-- Single-row cursor/gap state. `version` is a compare-and-swap counter:
-- every save requires the version it was loaded with, so two collector
-- instances (e.g. an overlapping deploy) detect each other instead of
-- silently interleaving writes. `writer_id` is diagnostic only.
CREATE TABLE ref_flow_collector_state (
    id         SMALLINT  PRIMARY KEY CHECK (id = 1),
    version    BIGINT    NOT NULL CHECK (version >= 1),
    writer_id  VARCHAR   NOT NULL,
    state      JSONB     NOT NULL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Requests sent to NearBlocks per UTC day. Charged before each attempt,
-- so the ledger is an upper bound of the provider's count. Never subject
-- to the retention DELETE: the rolling 31-day quota needs it.
CREATE TABLE ref_flow_ledger (
    day      DATE   PRIMARY KEY,
    requests BIGINT NOT NULL CHECK (requests >= 0)
);

-- One row per pass of a collection cycle (forward / gap / enrich).
CREATE TABLE ref_flow_runs (
    id          BIGSERIAL PRIMARY KEY,
    run_at      TIMESTAMP NOT NULL,
    -- SYNC: allowed values must match RunStream::from_str in
    -- crates/persistence/src/ref_flow/runs.rs
    stream      VARCHAR   NOT NULL CHECK (stream IN ('fwd', 'gap', 'enrich')),
    pages       INTEGER   NOT NULL CHECK (pages >= 0),
    receipts    INTEGER   NOT NULL CHECK (receipts >= 0),
    trades      INTEGER   NOT NULL CHECK (trades >= 0),
    from_time   TIMESTAMP NULL,
    to_time     TIMESTAMP NULL,
    credits     INTEGER   NOT NULL CHECK (credits >= 0),
    http429     INTEGER   NOT NULL DEFAULT 0 CHECK (http429 >= 0),
    failures    INTEGER   NOT NULL DEFAULT 0 CHECK (failures >= 0),
    note        VARCHAR   NOT NULL DEFAULT '',
    parse_stats JSONB     NULL
);
CREATE INDEX idx_ref_flow_runs_run_at ON ref_flow_runs (run_at);
