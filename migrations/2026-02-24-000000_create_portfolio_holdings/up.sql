CREATE TABLE portfolio_holdings (
    id SERIAL PRIMARY KEY,
    evaluation_period_id VARCHAR NOT NULL
        REFERENCES evaluation_periods(period_id),
    timestamp TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    token_holdings JSONB NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_portfolio_holdings_period_time
    ON portfolio_holdings(evaluation_period_id, timestamp DESC);
