-- Layer 3 of the 4-layer data-leakage defense for prediction_records.
--
-- Adds a CHECK constraint enforcing `created_at >= data_cutoff_time` for
-- ALL future INSERT/UPDATE paths (Diesel / raw SQL / psql / DBA / migration
-- backfill). Without this constraint, a DB-write-privileged attacker (or a
-- bug bypassing the Rust caller-side `try_new` guard) can inject look-ahead
-- bias by writing rows where `created_at < data_cutoff_time`, causing the
-- optimizer to read predictions that were not yet visible at `as_of`.
--
-- `NOT VALID` means existing rows are NOT validated at constraint creation
-- time, keeping this migration lock-minimal and safe for production rollout.
-- A follow-up migration (validate_prediction_invariant_check) quarantines
-- any existing violators and runs `VALIDATE CONSTRAINT` to enforce the
-- invariant on legacy rows as well.
ALTER TABLE prediction_records
    ADD CONSTRAINT created_at_geq_data_cutoff
    CHECK (created_at >= data_cutoff_time)
    NOT VALID;
