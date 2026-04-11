use std::time::{SystemTime, UNIX_EPOCH};

use grpc_auth::AuthError;
use jsonwebtoken::{Validation, decode, decode_header};
use serde::Deserialize;

use crate::jwks::{ACCEPTED_ALGORITHM, JwksCache};

/// Clock skew tolerance (seconds) for `exp`, `nbf`, and `iat` validation.
const LEEWAY_SECONDS: u64 = 60;

/// Maximum acceptable token age based on `iat`.
///
/// Google ID tokens are issued with a 1 hour `exp`, so legitimate tokens
/// will never be older than 1 hour. Tightening the ceiling to match reduces
/// the window in which a leaked token can be replayed before the `exp`
/// check catches up.
const MAX_TOKEN_AGE_SECONDS: u64 = 60 * 60;

/// Accepted issuer values for Google ID tokens.
const ACCEPTED_ISSUERS: &[&str] = &["https://accounts.google.com", "accounts.google.com"];

/// Subset of the Google ID token claim set that we actually consume.
///
/// Google sends `email_verified` as a boolean in id_tokens, though some legacy
/// flows send it as a string. We accept both via `deserialize_with`.
///
/// `iat` is required and used both as a spec claim (`required_spec_claims`)
/// and for the manual max-age check below.
#[derive(Debug, Deserialize)]
pub struct Claims {
    pub(crate) email: String,
    #[serde(deserialize_with = "deserialize_bool_or_string")]
    pub(crate) email_verified: bool,
    pub(crate) iat: u64,
}

fn deserialize_bool_or_string<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Unexpected};
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrString {
        Bool(bool),
        Str(String),
    }
    match BoolOrString::deserialize(deserializer)? {
        BoolOrString::Bool(b) => Ok(b),
        BoolOrString::Str(s) => match s.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            other => Err(de::Error::invalid_value(
                Unexpected::Str(other),
                &"true or false",
            )),
        },
    }
}

/// Validate a Google-issued ID token.
///
/// Checks performed:
/// - Signature algorithm is RS256
/// - Signature verifies against a key in the JWKS cache (looked up by `kid`)
/// - `iss` is one of Google's accepted values
/// - `aud` matches the configured client id
/// - `exp` is in the future (60 second leeway)
/// - `nbf`, when present, is not in the future (60 second leeway)
/// - `iat` is present and not older than [`MAX_TOKEN_AGE_SECONDS`] +
///   [`LEEWAY_SECONDS`] in the past (defence in depth against leaked tokens)
/// - `sub` and `iat` are present (`required_spec_claims`)
/// - `email_verified` is true
///
/// Returns the validated claims. Does not check whether the email is in the
/// user allowlist; that is the caller's responsibility.
///
/// # Replay protection
///
/// The validator does not track `jti` or maintain a nonce cache. A token
/// passing all checks here remains accepted until the earlier of its `exp`
/// or the [`MAX_TOKEN_AGE_SECONDS`] ceiling. See the "Threat model: token
/// replay" section on `web::serve` for the rationale and follow-up plan.
pub fn validate_id_token(
    token: &str,
    client_id: &str,
    jwks: &JwksCache,
) -> Result<Claims, AuthError> {
    // Fail-closed "auth disabled" state: if the operator started the process
    // without a `google_client_id`, reject every token unconditionally before
    // any parsing work. This is the runtime half of the contract documented
    // on `GoogleAuthenticator::new` — empty `client_id` is intentionally a
    // supported startup mode that guarantees no request can authenticate.
    // Do NOT change this to bail at construction time without updating the
    // authenticator doc and the `web::serve` threat model notes.
    if client_id.is_empty() {
        return Err(AuthError::InvalidToken(
            "auth disabled: client_id not configured".to_string(),
        ));
    }

    let header =
        decode_header(token).map_err(|e| AuthError::InvalidToken(format!("decode_header: {e}")))?;

    if header.alg != ACCEPTED_ALGORITHM {
        return Err(AuthError::InvalidToken(format!(
            "unsupported alg: {:?}",
            header.alg
        )));
    }

    let kid = header
        .kid
        .ok_or_else(|| AuthError::InvalidToken("missing kid".to_string()))?;

    let key = jwks.get(&kid).ok_or_else(|| {
        if jwks.is_empty() {
            AuthError::JwksUnavailable
        } else {
            AuthError::InvalidToken("unknown kid".to_string())
        }
    })?;

    let mut validation = Validation::new(ACCEPTED_ALGORITHM);
    validation.leeway = LEEWAY_SECONDS;
    validation.set_audience(&[client_id]);
    validation.set_issuer(ACCEPTED_ISSUERS);
    validation.validate_exp = true;
    validation.validate_nbf = true;
    validation.set_required_spec_claims(&["exp", "aud", "iss", "sub", "iat"]);

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| AuthError::InvalidToken(format!("decode: {e}")))?;

    // Defence-in-depth: enforce a hard upper bound on token age based on `iat`.
    //
    // `jsonwebtoken::Validation::leeway` (set above) only relaxes the spec
    // claim checks — `exp`, `nbf`, and the library's own "`iat` in the
    // future" check — by `LEEWAY_SECONDS`. The age ceiling enforced below
    // is *outside* the JWT spec, so the leeway configured on `Validation`
    // does not apply to it automatically. We therefore explicitly add the
    // same `LEEWAY_SECONDS` to both the future-`iat` guard and the
    // too-old-`iat` guard so a valid token generated right at the edge of
    // the allowed clock skew is not rejected here. This is deliberate and
    // is NOT a double count of the `Validation` leeway.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AuthError::InvalidToken("system clock before unix epoch".to_string()))?
        .as_secs();
    let iat = token_data.claims.iat;
    // Future iat beyond leeway (clock skew) is suspicious.
    if iat > now.saturating_add(LEEWAY_SECONDS) {
        return Err(AuthError::InvalidToken("iat is in the future".to_string()));
    }
    // Past iat older than MAX_TOKEN_AGE + leeway is rejected. `saturating_sub`
    // clamps `age` to 0 whenever `iat` is at most `LEEWAY_SECONDS` in the
    // future (already permitted by the guard above); 0 is trivially inside
    // the allowed window, which is the safe direction for an upper bound.
    let age = now.saturating_sub(iat);
    if age > MAX_TOKEN_AGE_SECONDS.saturating_add(LEEWAY_SECONDS) {
        return Err(AuthError::InvalidToken("token too old".to_string()));
    }

    if !token_data.claims.email_verified {
        return Err(AuthError::EmailNotVerified);
    }

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests;
