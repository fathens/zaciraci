-- get_spot_rates_at_time CTE クエリ最適化用の複合インデックス
-- DISTINCT ON (base_token) ... WHERE base_token = ANY(...) AND quote_token = ... AND timestamp <= ...
-- ORDER BY base_token, timestamp DESC に最適化
CREATE INDEX idx_token_rates_spot_rate_lookup
ON token_rates (base_token, quote_token, timestamp DESC);
