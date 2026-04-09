-- Defense-in-depth: enforce case-insensitive uniqueness at the DB level so
-- that direct SQL inserts (manual fixes, ad-hoc tooling) cannot bypass the
-- Email newtype's lowercase normalization and create two rows that resolve
-- to the same principal at the application layer.
CREATE UNIQUE INDEX authorized_users_email_lower_idx
    ON authorized_users (LOWER(email));
