-- Your SQL goes here
ALTER TABLE pool_info RENAME COLUMN updated_at TO timestamp;

-- ユニーク制約も更新
ALTER TABLE pool_info DROP CONSTRAINT pool_info_pool_id_updated_at_key;
ALTER TABLE pool_info ADD CONSTRAINT pool_info_pool_id_timestamp_key UNIQUE (pool_id, timestamp);
