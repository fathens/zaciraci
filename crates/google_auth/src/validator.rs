use grpc_auth::AuthError;
use jsonwebtoken::{Validation, decode, decode_header};
use serde::Deserialize;

use crate::jwks::{JwksCache, accepted_algorithm};

/// Clock skew tolerance (seconds) for `exp` and `nbf` validation.
const LEEWAY_SECONDS: u64 = 60;

/// Accepted issuer values for Google ID tokens.
const ACCEPTED_ISSUERS: &[&str] = &["https://accounts.google.com", "accounts.google.com"];

/// Subset of the Google ID token claim set that we actually consume.
///
/// Google sends `email_verified` as a boolean in id_tokens, though some legacy
/// flows send it as a string. We accept both via `deserialize_with`.
#[derive(Debug, Deserialize)]
pub struct Claims {
    pub email: String,
    #[serde(deserialize_with = "deserialize_bool_or_string")]
    pub email_verified: bool,
    #[serde(default)]
    pub sub: String,
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
/// - `email_verified` is true
///
/// Returns the validated claims. Does not check whether the email is in the
/// user allowlist; that is the caller's responsibility.
pub fn validate_id_token(
    token: &str,
    client_id: &str,
    jwks: &JwksCache,
) -> Result<Claims, AuthError> {
    if client_id.is_empty() {
        return Err(AuthError::InvalidToken(
            "client_id not configured".to_string(),
        ));
    }

    let header =
        decode_header(token).map_err(|e| AuthError::InvalidToken(format!("decode_header: {e}")))?;

    if header.alg != accepted_algorithm() {
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

    let mut validation = Validation::new(accepted_algorithm());
    validation.leeway = LEEWAY_SECONDS;
    validation.set_audience(&[client_id]);
    validation.set_issuer(ACCEPTED_ISSUERS);
    validation.validate_exp = true;
    validation.set_required_spec_claims(&["exp", "aud", "iss"]);

    let token_data = decode::<Claims>(token, &key, &validation)
        .map_err(|e| AuthError::InvalidToken(format!("decode: {e}")))?;

    if !token_data.claims.email_verified {
        return Err(AuthError::EmailNotVerified);
    }

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests;
