-- rate_calc_near カラムを追加（レート計算に使用した NEAR 量を記録）
-- BIGINT (i64) を使用：YoctoAmount の全範囲を NEAR に変換すると最大 49 bits 必要
ALTER TABLE token_rates ADD COLUMN rate_calc_near BIGINT;

-- 既存データには 100 (従来のデフォルト) を設定
UPDATE token_rates SET rate_calc_near = 100 WHERE rate_calc_near IS NULL;

-- NOT NULL 制約を追加
ALTER TABLE token_rates ALTER COLUMN rate_calc_near SET NOT NULL;

-- 新規データのデフォルト値は 10 (10%ルール)
ALTER TABLE token_rates ALTER COLUMN rate_calc_near SET DEFAULT 10;
