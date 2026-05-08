-- Pre-flight inspection for migration
-- 2026-05-07-000000_add_prediction_invariant_check.
--
-- Run BEFORE applying the migration:
--
--     psql "$DATABASE_URL" -f preflight.sql
--
-- Diesel does NOT execute this file (only up.sql / down.sql are picked up);
-- it is operator-run on purpose. The migration's `ADD CONSTRAINT ... CHECK`
-- will fail loudly if any violator remains, so a forgotten preflight turns
-- into a deploy-blocking error rather than silent data movement.
--
-- Expected output: 0 rows.
--
-- If rows are returned, those are existing prediction_records that violate
-- the data-leakage invariant `created_at >= data_cutoff_time`. Triage:
--
--   * Known test/dev data → DELETE the violators, then run the migration.
--   * Possible real look-ahead-bias incident → dump to CSV first
--     (`\copy (SELECT * FROM prediction_records WHERE NOT (created_at >=
--     data_cutoff_time)) TO 'violators.csv' CSV HEADER`), then DELETE.
--   * Data must be retained as-is → do NOT run the migration; fix the
--     underlying bug that produced the violators first.

SELECT
    id,
    token,
    quote_token,
    created_at,
    data_cutoff_time,
    target_time,
    (data_cutoff_time - created_at) AS leakage_window
FROM prediction_records
WHERE NOT (created_at >= data_cutoff_time)
ORDER BY created_at;
