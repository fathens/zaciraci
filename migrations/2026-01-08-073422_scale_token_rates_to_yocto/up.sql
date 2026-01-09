-- Scale existing rates to yocto units (multiply by 10^24)
-- Old: rate = tokens_yocto / (100 * 10^24)
-- New: rate = tokens_yocto / 100 (yocto tokens per 1 NEAR)
UPDATE token_rates SET rate = rate * 1000000000000000000000000;

-- Add comment to document the unit change
COMMENT ON COLUMN token_rates.rate IS 'Exchange rate in yocto units: yocto tokens received per 1 NEAR';
