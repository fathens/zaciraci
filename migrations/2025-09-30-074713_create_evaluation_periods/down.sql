-- インデックスの削除
DROP INDEX IF EXISTS idx_evaluation_periods_period_id;
DROP INDEX IF EXISTS idx_evaluation_periods_start_time;

-- テーブルの削除
DROP TABLE IF EXISTS evaluation_periods;