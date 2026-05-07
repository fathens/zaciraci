-- Rollback notes:
-- (1) ALTER TABLE ... VALIDATE CONSTRAINT is one-way at the SQL level;
--     PostgreSQL has no UNVALIDATE. To revert to NOT VALID semantics we
--     drop and re-add the constraint as NOT VALID.
-- (2) Quarantined rows are NOT moved back into prediction_records by this
--     down migration; they remain in prediction_records_quarantine for
--     forensic review. The quarantine table itself is dropped.

ALTER TABLE prediction_records
    DROP CONSTRAINT created_at_geq_data_cutoff;

ALTER TABLE prediction_records
    ADD CONSTRAINT created_at_geq_data_cutoff
    CHECK (created_at >= data_cutoff_time)
    NOT VALID;

DROP TABLE prediction_records_quarantine;
