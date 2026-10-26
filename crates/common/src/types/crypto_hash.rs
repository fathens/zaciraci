use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

/// Byte length of every NEAR `CryptoHash` (transaction hash, receipt id,
/// block hash).
const HASH_BYTES: usize = 32;

/// Upper bound on the encoded text we are willing to decode. A 32-byte
/// value never needs more than 44 base58 characters; anything longer is
/// rejected before decoding so untrusted input cannot make us do
/// unbounded work.
const MAX_ENCODED_LEN: usize = 64;

/// Error returned when a string is not a base58-encoded 32-byte hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParseHashError {
    #[error("hash is empty")]
    Empty,
    #[error("hash is {0} characters long, longer than the {MAX_ENCODED_LEN} allowed")]
    TooLong(usize),
    #[error("hash contains a character outside the base58 alphabet")]
    InvalidCharacter,
    #[error("hash decodes to {0} bytes, expected {HASH_BYTES}")]
    InvalidLength(usize),
}

/// Validate that `input` is the base58 encoding of exactly 32 bytes.
///
/// No trimming or normalization is applied: whitespace and any character
/// outside the Bitcoin base58 alphabet is rejected. This is deliberate,
/// because these values are embedded verbatim into external URLs and used
/// as database keys, so the stored text must be byte-for-byte canonical.
fn validate_base58_hash(input: &str) -> Result<(), ParseHashError> {
    if input.is_empty() {
        return Err(ParseHashError::Empty);
    }
    if input.len() > MAX_ENCODED_LEN {
        return Err(ParseHashError::TooLong(input.len()));
    }
    let decoded = bs58::decode(input)
        .into_vec()
        .map_err(|_| ParseHashError::InvalidCharacter)?;
    if decoded.len() != HASH_BYTES {
        return Err(ParseHashError::InvalidLength(decoded.len()));
    }
    Ok(())
}

macro_rules! base58_hash_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        ///
        /// The wrapped string is guaranteed to be the base58 encoding of
        /// exactly 32 bytes, with no surrounding whitespace. Because
        /// construction validates the alphabet, a value of this type can be
        /// safely interpolated into a URL path segment: it can never contain
        /// `/`, `?`, `#`, `.` or any other delimiter.
        ///
        /// `Display` renders the canonical text (hashes are public on-chain
        /// identifiers, not secrets).
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validate `input` and wrap it.
            pub fn new(input: &str) -> Result<Self, ParseHashError> {
                validate_base58_hash(input)?;
                Ok(Self(input.to_owned()))
            }

            /// The canonical base58 text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl FromStr for $name {
            type Err = ParseHashError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::new(s)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let raw = String::deserialize(deserializer)?;
                Self::new(&raw).map_err(serde::de::Error::custom)
            }
        }
    };
}

base58_hash_newtype! {
    /// A NEAR transaction hash.
    TxHash
}

base58_hash_newtype! {
    /// A NEAR receipt id.
    ReceiptId
}

#[cfg(test)]
mod tests;
