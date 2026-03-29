-- cleanup DELETE (WHERE timestamp < $1) の高速化
CREATE INDEX idx_pool_info_timestamp ON pool_info (timestamp);
CREATE INDEX idx_portfolio_holdings_timestamp ON portfolio_holdings (timestamp);

-- get_latest_fresh_predictions, get_previous_evaluated の複合フィルタ最適化
CREATE INDEX idx_prediction_records_token_target
ON prediction_records (token, target_time DESC);
