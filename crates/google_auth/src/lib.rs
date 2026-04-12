#![deny(warnings)]

// Only `GoogleAuthenticator` is intentionally part of this crate's public
// surface; every other module is an implementation detail. Keeping them
// `pub(crate)` prevents downstream code (and in particular any
// test-only constructors like `JwksCache::from_keys` /
// `UserCache::from_entries`) from being reached from outside the crate,
// which in turn keeps the attack surface minimal and makes future
// internal refactors cheaper.
pub(crate) mod authenticator;
pub(crate) mod jwks;
pub(crate) mod user_cache;
pub(crate) mod validator;

pub use authenticator::GoogleAuthenticator;
