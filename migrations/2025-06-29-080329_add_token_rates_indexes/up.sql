-- ボラティリティ計算クエリを最適化するための複合インデックス
CREATE INDEX idx_token_rates_volatility_query 
ON token_rates (quote_token, timestamp, base_token) 
WHERE rate != 0;

-- 時系列クエリ用のインデックス
CREATE INDEX idx_token_rates_timestamp ON token_rates (timestamp);

-- base_tokenでの検索用インデックス
CREATE INDEX idx_token_rates_base_token ON token_rates (base_token);