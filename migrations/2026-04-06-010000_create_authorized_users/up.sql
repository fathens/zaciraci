CREATE TABLE authorized_users (
    id SERIAL PRIMARY KEY,
    email VARCHAR NOT NULL
        -- Defense-in-depth: require email to already be lowercased at insert
        -- time so direct SQL cannot create rows that the application's
        -- `Email` newtype would reject at read time. Combined with the
        -- `LOWER(email)` UNIQUE index below, this makes it impossible for the
        -- table to hold two distinct-case rows for the same principal.
        CHECK (email = LOWER(email)),
    -- SYNC: allowed values must match Role::from_str in
    -- crates/common/src/types/role.rs
    role VARCHAR NOT NULL DEFAULT 'reader'
        CHECK (role IN ('reader', 'writer')),
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Defense-in-depth: enforce case-insensitive uniqueness at the DB level so
-- that direct SQL inserts (manual fixes, ad-hoc tooling) cannot bypass the
-- Email newtype's lowercase normalization and create two rows that resolve
-- to the same principal at the application layer.
CREATE UNIQUE INDEX authorized_users_email_lower_idx
    ON authorized_users (LOWER(email));
