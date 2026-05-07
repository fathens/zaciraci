-- Phase 2 of the data-leakage 4-layer defense for prediction_records
-- (Layer 3: DB CHECK constraint becomes fully enforced via VALIDATE):
-- quarantine existing violators, delete them from the live table, then
-- VALIDATE the CHECK constraint added in the previous migration so it
-- enforces the invariant on legacy rows as well.
--
-- Quarantine preserves forensic / audit trail for any historical
-- look-ahead-bias incidents (a bare DELETE would erase evidence).
-- Retention: 365 days recommended (covers all evaluation cycles that
-- might still reference these rows for postmortem).

-- Composite primary key (id, quarantined_at) supports re-quarantine after
-- a hypothetical down/up cycle: the same prediction_records.id could be
-- archived multiple times if the constraint is dropped, violators reappear,
-- then the constraint is re-validated.
--
-- IF NOT EXISTS guards `diesel migration redo`: the down migration deliberately
-- preserves the quarantine table for forensic retention (see down.sql comment),
-- so a subsequent up replay must accept the existing table without DDL conflict.
-- INSERT below remains safe under replay because the composite PK includes
-- quarantined_at DEFAULT NOW(), so re-archiving the same id produces a new row
-- rather than a unique-violation.
--
-- TODO(security follow-up): once migration/runtime DB role separation is in
-- place, REVOKE INSERT/UPDATE/DELETE/TRUNCATE on this table from the runtime
-- role so forensic data cannot be tampered with by application code paths.
CREATE TABLE IF NOT EXISTS prediction_records_quarantine (
    id              INTEGER          NOT NULL,
    token           VARCHAR          NOT NULL,
    quote_token     VARCHAR          NOT NULL,
    predicted_price NUMERIC          NOT NULL,
    data_cutoff_time TIMESTAMP       NOT NULL,
    target_time     TIMESTAMP        NOT NULL,
    actual_price    NUMERIC,
    mape            DOUBLE PRECISION,
    absolute_error  NUMERIC,
    evaluated_at    TIMESTAMP,
    created_at      TIMESTAMP        NOT NULL,
    quarantined_at  TIMESTAMP        NOT NULL DEFAULT NOW(),
    PRIMARY KEY (id, quarantined_at)
);

COMMENT ON TABLE prediction_records_quarantine IS
    'Forensic archive of prediction_records rows that violated created_at >= data_cutoff_time. Retention: 365 days recommended; manual cleanup by ops.';

-- Move violators to the quarantine table (preserves all original columns).
INSERT INTO prediction_records_quarantine (
    id, token, quote_token, predicted_price, data_cutoff_time, target_time,
    actual_price, mape, absolute_error, evaluated_at, created_at
)
SELECT
    id, token, quote_token, predicted_price, data_cutoff_time, target_time,
    actual_price, mape, absolute_error, evaluated_at, created_at
FROM prediction_records
WHERE NOT (created_at >= data_cutoff_time);

DELETE FROM prediction_records
WHERE NOT (created_at >= data_cutoff_time);

-- All violators removed; safe to validate the constraint on existing rows.
-- Layer 3 (DB CHECK NOT VALID → fully enforced): from this point on, every
-- INSERT/UPDATE path (Diesel / raw SQL / psql / DBA / migration backfill)
-- is rejected by Postgres if it violates `created_at >= data_cutoff_time`.
ALTER TABLE prediction_records
    VALIDATE CONSTRAINT created_at_geq_data_cutoff;
