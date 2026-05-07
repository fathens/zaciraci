-- IF EXISTS keeps `diesel migration redo` idempotent: the up migration
-- creates the constraint with a stable name, so a redo cycle that re-enters
-- this down.sql after the constraint was already dropped (e.g., by a
-- subsequent partial revert) must not error out. The sibling migration
-- (`2026-05-07-000001_validate_prediction_invariant_check/down.sql`)
-- already uses IF EXISTS for the same reason; aligning both removes the
-- asymmetry.
ALTER TABLE prediction_records
    DROP CONSTRAINT IF EXISTS created_at_geq_data_cutoff;
