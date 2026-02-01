-- Drop indexes
DROP INDEX IF EXISTS idx_trade_transactions_tokens;
DROP INDEX IF EXISTS idx_trade_transactions_batch_id;
DROP INDEX IF EXISTS idx_trade_transactions_timestamp;

-- Drop trade_transactions table
DROP TABLE IF EXISTS trade_transactions;
