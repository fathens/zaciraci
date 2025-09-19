-- Create trade_transactions table for recording successful trades
CREATE TABLE trade_transactions (
    tx_id VARCHAR PRIMARY KEY,                  -- NEAR transaction hash (unique)
    trade_batch_id VARCHAR NOT NULL,           -- ID for grouping simultaneous trades (UUID)
    from_token VARCHAR NOT NULL,               -- Source token identifier
    from_amount NUMERIC(39, 0) NOT NULL,       -- Source token amount
    to_token VARCHAR NOT NULL,                 -- Destination token identifier
    to_amount NUMERIC(39, 0) NOT NULL,         -- Destination token amount
    price_yocto_near NUMERIC(39, 0) NOT NULL,  -- Price in yoctoNEAR
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for search performance
CREATE INDEX idx_trade_transactions_timestamp ON trade_transactions(timestamp);
CREATE INDEX idx_trade_transactions_batch_id ON trade_transactions(trade_batch_id);
CREATE INDEX idx_trade_transactions_tokens ON trade_transactions(from_token, to_token);
