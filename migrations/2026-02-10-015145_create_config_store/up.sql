CREATE TABLE config_store (
    instance_id VARCHAR     NOT NULL DEFAULT '*',
    key         VARCHAR     NOT NULL,
    value       TEXT        NOT NULL,
    description TEXT,
    updated_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (instance_id, key)
);

CREATE TABLE config_store_history (
    id          SERIAL      PRIMARY KEY,
    instance_id VARCHAR     NOT NULL DEFAULT '*',
    key         VARCHAR     NOT NULL,
    old_value   TEXT,
    new_value   TEXT        NOT NULL,
    changed_at  TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_config_store_history_key
    ON config_store_history (key, changed_at DESC);
