-- インデックスを削除
DROP INDEX IF EXISTS idx_trade_transactions_evaluation_period_id;

-- 外部キー制約を削除
ALTER TABLE trade_transactions
DROP CONSTRAINT IF EXISTS fk_trade_transactions_evaluation_period;

-- カラムを削除
ALTER TABLE trade_transactions
DROP COLUMN IF EXISTS evaluation_period_id;