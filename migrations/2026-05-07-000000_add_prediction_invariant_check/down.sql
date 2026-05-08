-- IF EXISTS keeps `diesel migration redo` idempotent: a redo cycle that
-- re-enters this down.sql after the constraint was already dropped must
-- not error out.
ALTER TABLE prediction_records
    DROP CONSTRAINT IF EXISTS created_at_geq_data_cutoff;
