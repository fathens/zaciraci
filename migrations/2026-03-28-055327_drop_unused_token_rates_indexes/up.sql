-- 未使用インデックスの削除
-- idx_token_rates_volatility_query: 300 MB, 0スキャン（部分インデックス WHERE rate != 0 の制約で使われず）
-- idx_token_rates_base_token: 31 MB, 2スキャン（idx_token_rates_spot_rate_lookup がカバー）
DROP INDEX IF EXISTS idx_token_rates_volatility_query;
DROP INDEX IF EXISTS idx_token_rates_base_token;
