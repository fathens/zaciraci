-- This file should undo anything in `up.sql`
ALTER TABLE pool_info RENAME COLUMN timestamp TO updated_at;
-- ユニーク制約も元に戻す
ALTER TABLE pool_info DROP CONSTRAINT pool_info_pool_id_timestamp_key;
ALTER TABLE pool_info ADD CONSTRAINT pool_info_pool_id_updated_at_key UNIQUE (pool_id, updated_at);
