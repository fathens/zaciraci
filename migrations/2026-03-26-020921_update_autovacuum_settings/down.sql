-- pool_info: 旧設定に戻す（2025-10-16-003351 で設定した値）
ALTER TABLE pool_info SET (
  autovacuum_vacuum_scale_factor = 0.05,
  autovacuum_vacuum_threshold = 1000
);
ALTER TABLE pool_info RESET (autovacuum_vacuum_cost_delay);

-- token_rates: per-table 設定を削除（デフォルトに戻す）
ALTER TABLE token_rates RESET (
  autovacuum_vacuum_scale_factor,
  autovacuum_vacuum_threshold
);
