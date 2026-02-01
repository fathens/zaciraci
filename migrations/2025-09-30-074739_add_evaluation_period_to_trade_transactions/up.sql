-- trade_transactionsテーブルに評価期間IDカラムを追加
ALTER TABLE trade_transactions
ADD COLUMN evaluation_period_id VARCHAR;

-- 外部キー制約を追加
ALTER TABLE trade_transactions
ADD CONSTRAINT fk_trade_transactions_evaluation_period
FOREIGN KEY (evaluation_period_id)
REFERENCES evaluation_periods(period_id);

-- インデックスを追加
CREATE INDEX idx_trade_transactions_evaluation_period_id
ON trade_transactions(evaluation_period_id);