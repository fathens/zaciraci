-- Fix index column order to match get_all_latest_rates query:
--   WHERE quote_token = $1 ORDER BY base_token, timestamp DESC
-- Previous order (base_token, quote_token, timestamp DESC) cannot use
-- quote_token equality filter efficiently → full table scan.
-- New order (quote_token, base_token, timestamp DESC) allows index scan
-- on quote_token equality, then base_token + timestamp DESC for DISTINCT ON.
DROP INDEX IF EXISTS idx_token_rates_spot_rate_lookup;

CREATE INDEX idx_token_rates_spot_rate_lookup
ON token_rates (quote_token, base_token, timestamp DESC);
