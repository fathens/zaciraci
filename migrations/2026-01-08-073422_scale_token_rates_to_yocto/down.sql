-- Revert rates to original scale (divide by 10^24)
UPDATE token_rates SET rate = rate / 1000000000000000000000000;

-- Remove comment
COMMENT ON COLUMN token_rates.rate IS NULL;
