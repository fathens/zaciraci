CREATE INDEX idx_token_rates_volatility_query
ON token_rates (quote_token, timestamp, base_token)
WHERE rate != 0;

CREATE INDEX idx_token_rates_base_token ON token_rates (base_token);
