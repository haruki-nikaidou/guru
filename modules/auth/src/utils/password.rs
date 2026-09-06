//! Password hashing and verification.
//!
//! A small strategy trait ([`PasswordAlgorithm`]) abstracts the concrete
//! key-derivation function so the services depend on the capability, not on a
//! specific crate. The only implementation shipped is [`Argon2PasswordAlgorithm`]
//! (Argon2id with library defaults); no other algorithms are supported.

use argon2::password_hash::{Error as Argon2Error, SaltString, rand_core::OsRng};
use argon2::{PasswordHash, PasswordHasher, PasswordVerifier};

/// A password hashing strategy: derive a verifier-encoded hash from a plaintext
/// password and check a candidate password against a stored hash.
pub trait PasswordAlgorithm {
    /// Hash `password`, returning the PHC-string encoding to persist.
    fn hash_password(&self, password: &str) -> Result<String, PasswordHashError>;
    /// Return `true` iff `password` matches the previously stored `hash`.
    ///
    /// Any parse/verification failure (including a malformed `hash`) yields
    /// `false`; it never surfaces an error to the caller.
    fn verify_password(&self, password: &str, hash: &str) -> bool;
}

/// Argon2id password hasher using the crate's default parameters.
///
/// `argon2::Argon2<'static>` is both `Clone` and `Default`, so this struct is
/// cheaply cloneable and can be embedded directly in a service.
#[derive(Clone, Default)]
pub struct Argon2PasswordAlgorithm {
    config: argon2::Argon2<'static>,
}

impl PasswordAlgorithm for Argon2PasswordAlgorithm {
    fn hash_password(&self, password: &str) -> Result<String, PasswordHashError> {
        let salt = SaltString::generate(&mut OsRng);
        let hash = self
            .config
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        Ok(hash)
    }

    fn verify_password(&self, password: &str, hash: &str) -> bool {
        let parsed = match PasswordHash::new(hash) {
            Ok(parsed) => parsed,
            Err(error) => {
                tracing::warn!(%error, "failed to parse stored password hash");
                return false;
            }
        };
        match self.config.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => true,
            Err(Argon2Error::Password) => false,
            Err(error) => {
                tracing::warn!(%error, "password verification failed unexpectedly");
                false
            }
        }
    }
}

/// Errors that can arise while hashing a password.
#[derive(Debug, thiserror::Error)]
pub enum PasswordHashError {
    /// The Argon2 configuration itself is invalid.
    #[error("invalid argon2 configuration")]
    InvalidConfig,
    /// Encoding the derived hash into its PHC string form failed.
    #[error("failed to encode password hash")]
    EncodingFailed,
    /// Decoding an input (salt/parameters) failed.
    #[error("failed to decode password hashing input")]
    DecodeFailed,
    /// Any other Argon2 failure.
    #[error("unknown password hashing error")]
    UnknownError,
}

impl From<Argon2Error> for PasswordHashError {
    fn from(error: Argon2Error) -> Self {
        match error {
            Argon2Error::Algorithm | Argon2Error::Version | Argon2Error::ParamNameInvalid => {
                Self::InvalidConfig
            }
            Argon2Error::B64Encoding(_)
            | Argon2Error::PhcStringField
            | Argon2Error::PhcStringTrailingData => Self::EncodingFailed,
            Argon2Error::SaltInvalid(_) | Argon2Error::ParamValueInvalid(_) => Self::DecodeFailed,
            _ => Self::UnknownError,
        }
    }
}
