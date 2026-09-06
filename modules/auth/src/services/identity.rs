//! The authenticated principal shared across services and the transport edge.

use crate::entities::surreal::account::{AccountId, AccountRole};
use crate::utils::rbac::Permission;

/// An authenticated caller: which account, its current role, and how it proved
/// its identity (a human session or a machine API key).
#[derive(Clone, Debug)]
pub struct Identity {
    pub account_id: AccountId,
    pub role: AccountRole,
    pub kind: IdentityKind,
}

/// How an [`Identity`] was established.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityKind {
    /// A human logged in with email + password, carrying a session id.
    Session,
    /// A machine presenting an API key secret.
    ApiKey,
}

impl Identity {
    /// Authorize a privileged auth-module operation.
    ///
    /// Every [`Permission`] in this module is human-only: API-key identities are
    /// rejected outright regardless of the owning account's role, so a server
    /// credential can never drive account/key management here. Otherwise the
    /// role's capability matrix decides.
    pub fn ensure(&self, permission: Permission) -> Result<(), wakuwaku::Error> {
        if self.kind == IdentityKind::ApiKey {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        if self.role.can(permission) {
            Ok(())
        } else {
            Err(wakuwaku::Error::PermissionsDenied)
        }
    }

    /// Require that this identity is a human session (used for self-service
    /// operations like changing one's own password or email).
    pub fn require_human(&self) -> Result<(), wakuwaku::Error> {
        if self.kind == IdentityKind::Session {
            Ok(())
        } else {
            Err(wakuwaku::Error::PermissionsDenied)
        }
    }
}
