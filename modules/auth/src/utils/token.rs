//! Opaque credential generation and digesting.
//!
//! Session tokens and API-key secrets are high-entropy random strings; only
//! their SHA-256 digest is persisted for API keys, while the session token is
//! itself the session record key. All helpers here are pure and hold no state.

use rand::Rng;
use rand::distr::Alphanumeric;
use sha2::{Digest, Sha256};

/// Number of random alphanumeric characters in an opaque credential.
///
/// 43 base-62 characters carry ~256 bits of entropy.
const TOKEN_LEN: usize = 43;

/// Prefix identifying an API-key secret (`gk` = "guru key").
const API_KEY_PREFIX: &str = "gk_";

/// Prefix identifying a worker's dynamic refresh key (`gr` = "guru refresh").
const REFRESH_KEY_PREFIX: &str = "gr_";

/// Generate a fresh opaque session token (~256 bits of entropy).
pub fn generate_session_token() -> String {
    random_alphanumeric(TOKEN_LEN)
}

/// Generate a fresh API-key secret: the `gk_` prefix plus ~256 bits of entropy.
pub fn generate_api_key_secret() -> String {
    let mut secret = String::with_capacity(API_KEY_PREFIX.len().saturating_add(TOKEN_LEN));
    secret.push_str(API_KEY_PREFIX);
    secret.push_str(&random_alphanumeric(TOKEN_LEN));
    secret
}

/// Generate a fresh dynamic refresh key for a worker session: the `gr_` prefix
/// plus ~256 bits of entropy. Only its digest is persisted.
pub fn generate_refresh_key() -> String {
    let mut secret = String::with_capacity(REFRESH_KEY_PREFIX.len().saturating_add(TOKEN_LEN));
    secret.push_str(REFRESH_KEY_PREFIX);
    secret.push_str(&random_alphanumeric(TOKEN_LEN));
    secret
}

/// Lowercase hexadecimal SHA-256 digest of `input`.
pub fn sha256_hex(input: &str) -> String {
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

/// Collect `len` random alphanumeric characters from the thread-local RNG.
fn random_alphanumeric(len: usize) -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}
