-- Tune autovacuum settings for pool_info table
-- This ensures autovacuum runs after each cleanup_old_records execution
--
-- Background:
-- - pool_info has ~65,940 live rows
-- - cleanup_old_records deletes 6,594 rows every 5 minutes
-- - Default threshold: 50 + 0.2 * 65,940 = 13,238 rows (requires 2-3 cleanups)
-- - New threshold: 1000 + 0.05 * 65,940 = 4,297 rows (triggers after 1 cleanup)

ALTER TABLE pool_info SET (
  autovacuum_vacuum_scale_factor = 0.05,
  autovacuum_vacuum_threshold = 1000
);
