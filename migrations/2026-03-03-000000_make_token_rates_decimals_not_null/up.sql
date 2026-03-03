DELETE FROM token_rates WHERE decimals IS NULL;
ALTER TABLE token_rates ALTER COLUMN decimals SET NOT NULL;
