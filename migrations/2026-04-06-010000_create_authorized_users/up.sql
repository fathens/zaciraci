CREATE TABLE authorized_users (
    id SERIAL PRIMARY KEY,
    email VARCHAR NOT NULL UNIQUE,
    role VARCHAR NOT NULL DEFAULT 'reader'
        CHECK (role IN ('reader', 'writer')),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
