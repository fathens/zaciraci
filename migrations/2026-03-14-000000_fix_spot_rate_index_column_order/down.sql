-- Revert to original column order (base_token, quote_token, timestamp DESC)
DROP INDEX IF EXISTS idx_token_rates_spot_rate_lookup;

CREATE INDEX idx_token_rates_spot_rate_lookup
ON token_rates (base_token, quote_token, timestamp DESC);
