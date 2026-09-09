//! Worker registration and acknowledgement.
//!
//! A worker holds its dynamic refresh key in memory only; the master stores just the
//! digest. Every registration rotates the key and bumps the generation, so a worker
//! restart is visible to the master and the previous session's streams die.

use crate::entities::surreal::revision::{FindServerConfigRevision, PruneServerRevisionsBelow};
use crate::entities::surreal::server::{
    FindServerById, FindServerByRefreshKeyDigest, MarkServerApplied, RotateServerRefreshKey,
    ServerId, SetServerApplyError,
};
use crate::services::OrchestrationError;
use crate::services::watch::{SessionLease, WatchHub};
use crate::utils::ids::record_key;
use auth::services::identity::Identity;
use auth::utils::rbac::Permission;
use auth::utils::token::{generate_refresh_key, sha256_hex};
use chrono::Utc;
use kanau::processor::Processor;
use wakuwaku::surreal::SurrealProcessor;

/// An authenticated worker: which server, and which refresh-key generation it holds.
#[derive(Clone, Debug)]
pub struct AgentIdentity {
    pub server: ServerId,
    pub generation: i64,
}

#[derive(Clone)]
pub struct AgentService {
    pub db: SurrealProcessor,
    pub hub: WatchHub,
    pub lease: SessionLease,
}

pub struct RegisterWorker {
    pub actor: Identity,
    pub server_id: ServerId,
    pub running_revision: i64,
}

impl Processor<RegisterWorker> for AgentService {
    /// The plaintext refresh key; only its digest is stored.
    type Output = String;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:RegisterWorker", skip_all, err)]
    async fn process(&self, input: RegisterWorker) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ServerCall)?;
        let server = self
            .db
            .process(FindServerById {
                id: input.server_id.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;

        let secret = generate_refresh_key();
        let now = Utc::now();
        // Refused while another session still heartbeats: a second worker pointed at
        // the same server must not be able to take it over just by reconnecting.
        let generation = self
            .db
            .process(RotateServerRefreshKey {
                server: server.id.clone(),
                digest: sha256_hex(&secret),
                now,
                lease_until: self.lease.until(now),
            })
            .await?
            .ok_or_else(|| {
                OrchestrationError::Conflict(
                    "another worker session is live for this server".into(),
                )
            })?
            .refresh_key_generation;

        // Reconcile what the worker reports it is actually running.
        if input.running_revision > 0 {
            let known = self
                .db
                .process(FindServerConfigRevision {
                    server: server.id.clone(),
                    revision: input.running_revision,
                })
                .await?;
            match known {
                Some(_) => {
                    self.db
                        .process(MarkServerApplied {
                            server: server.id.clone(),
                            revision: input.running_revision,
                        })
                        .await?;
                    self.db
                        .process(PruneServerRevisionsBelow {
                            server: server.id.clone(),
                            revision: input.running_revision,
                        })
                        .await?;
                }
                None => {
                    self.db
                        .process(SetServerApplyError {
                            server: server.id.clone(),
                            error: format!("running revision {} unknown", input.running_revision),
                        })
                        .await?;
                }
            }
        }

        self.hub.supersede(&record_key(&server.id.0), generation);
        Ok(secret)
    }
}

pub struct AuthenticateRefreshKey {
    pub secret: String,
}

impl Processor<AuthenticateRefreshKey> for AgentService {
    type Output = Option<AgentIdentity>;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:AuthenticateRefreshKey", skip_all, err)]
    async fn process(&self, input: AuthenticateRefreshKey) -> Result<Self::Output, Self::Error> {
        let server = self
            .db
            .process(FindServerByRefreshKeyDigest {
                digest: sha256_hex(&input.secret),
            })
            .await?;
        Ok(server.map(|server| AgentIdentity {
            generation: server.refresh_key_generation,
            server: server.id,
        }))
    }
}

pub struct AckConfig {
    pub agent: AgentIdentity,
    pub revision: i64,
    pub error: Option<String>,
}

impl Processor<AckConfig> for AgentService {
    type Output = ();
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:AckConfig", skip_all, err)]
    async fn process(&self, input: AckConfig) -> Result<Self::Output, Self::Error> {
        let server = self
            .db
            .process(FindServerById {
                id: input.agent.server.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        if server.refresh_key_generation != input.agent.generation {
            return Err(OrchestrationError::PermissionDenied);
        }
        match input.error {
            Some(error) => {
                self.db
                    .process(SetServerApplyError {
                        server: server.id,
                        error,
                    })
                    .await?;
            }
            None => {
                self.db
                    .process(FindServerConfigRevision {
                        server: server.id.clone(),
                        revision: input.revision,
                    })
                    .await?
                    .ok_or_else(|| OrchestrationError::Invalid("unknown revision".into()))?;
                self.db
                    .process(MarkServerApplied {
                        server: server.id.clone(),
                        revision: input.revision,
                    })
                    .await?;
                self.db
                    .process(PruneServerRevisionsBelow {
                        server: server.id,
                        revision: input.revision,
                    })
                    .await?;
            }
        }
        Ok(())
    }
}
