-- Add decimals column to token_rates table
-- Nullable to support legacy records that don't have decimals information
ALTER TABLE token_rates ADD COLUMN decimals SMALLINT;

COMMENT ON COLUMN token_rates.decimals IS
  'Token decimals (e.g., 6 for USDT, 18 for ETH, 24 for NEAR). NULL for legacy records.';
