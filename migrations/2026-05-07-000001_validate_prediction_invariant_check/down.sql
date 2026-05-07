-- Rollback notes:
-- (1) ALTER TABLE ... VALIDATE CONSTRAINT is one-way at the SQL level;
--     PostgreSQL has no UNVALIDATE. To revert to NOT VALID semantics we
--     drop and re-add the constraint as NOT VALID.
-- (2) Quarantined rows are NOT moved back into prediction_records by this
--     down migration; they remain in prediction_records_quarantine for
--     forensic review.
-- (3) The quarantine table itself is **deliberately NOT dropped** here.
--     COMMENT ON TABLE in up.sql declares 365-day retention for forensic
--     look-ahead-bias incident analysis, and the up migration's INSERT
--     turned violators into the only surviving copy of those rows. A bare
--     DROP TABLE here would silently destroy that forensic record on every
--     `diesel migration redo`. Operators who actually want to remove the
--     archive must do so manually after the retention window has passed.
--     The companion up.sql uses CREATE TABLE IF NOT EXISTS so that a
--     subsequent migration replay sees the surviving table without DDL
--     conflict.

ALTER TABLE prediction_records
    DROP CONSTRAINT IF EXISTS created_at_geq_data_cutoff;

ALTER TABLE prediction_records
    ADD CONSTRAINT created_at_geq_data_cutoff
    CHECK (created_at >= data_cutoff_time)
    NOT VALID;
