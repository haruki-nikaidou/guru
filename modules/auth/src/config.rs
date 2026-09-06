//! Module configuration.
//!
//! The config-store/Redis cache infrastructure does not exist yet in this
//! workspace, so services hold an [`AuthConfig`] value constructed via
//! [`Default`]. When a shared config store lands, bind this struct to a stable
//! key and load it through the cache helpers instead of constructing defaults.

use serde::{Deserialize, Serialize};

/// Tunable authentication settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// How long a session may sit idle (no activity) before it is treated as
    /// expired and rejected, in seconds. Sessions slide on each authenticated
    /// request.
    pub session_idle_ttl_secs: i64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            // One week of idle time.
            session_idle_ttl_secs: 7 * 24 * 3600,
        }
    }
}
