-- pool_info: 定常 ~20M 行、時間ベース retention による日次バッチ削除に合わせてチューニング
-- 旧設定（2025-10-16-003351）: scale_factor=0.05, threshold=1000（件数ベース retention 前提）
ALTER TABLE pool_info SET (
  autovacuum_vacuum_scale_factor = 0.01,
  autovacuum_vacuum_threshold = 50000,
  autovacuum_vacuum_cost_delay = 0
);

-- token_rates: 定常 ~4.4M 行、90日 retention による日次バッチ削除
ALTER TABLE token_rates SET (
  autovacuum_vacuum_scale_factor = 0.02,
  autovacuum_vacuum_threshold = 10000
);
