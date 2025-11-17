-- Revert pool_info autovacuum settings to defaults
ALTER TABLE pool_info RESET (
  autovacuum_vacuum_scale_factor,
  autovacuum_vacuum_threshold
);
