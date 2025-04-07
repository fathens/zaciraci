-- Your SQL goes here
CREATE TABLE pool_info (
    id SERIAL PRIMARY KEY,
    pool_id INT NOT NULL,
    pool_kind VARCHAR NOT NULL,
    token_account_ids JSONB NOT NULL,
    amounts JSONB NOT NULL,
    total_fee INT NOT NULL,
    shares_total_supply JSONB NOT NULL,
    amp BIGINT NOT NULL,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    
    -- 効率的なクエリのための複合インデックス
    UNIQUE (pool_id, updated_at)
);
