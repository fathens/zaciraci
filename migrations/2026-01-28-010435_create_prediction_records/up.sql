CREATE TABLE prediction_records (
    id SERIAL PRIMARY KEY,
    evaluation_period_id VARCHAR NOT NULL,
    token VARCHAR NOT NULL,
    quote_token VARCHAR NOT NULL,
    predicted_price NUMERIC NOT NULL,
    prediction_time TIMESTAMP NOT NULL,
    target_time TIMESTAMP NOT NULL,
    actual_price NUMERIC,
    mape DOUBLE PRECISION,
    absolute_error NUMERIC,
    evaluated_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_prediction_records_target ON prediction_records(target_time);
CREATE INDEX idx_prediction_records_evaluated ON prediction_records(evaluated_at);
