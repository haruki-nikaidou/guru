//! Worker registration and acknowledgement.
//!
//! A worker holds its dynamic refresh key in memory only; the master stores just the
//! digest. Every registration rotates the key and bumps the generation, so a worker
//! restart is visible to the master and the previous session's streams die.

use crate::entities::surreal::server::{
    FindServerById, FindServerByRefreshKeyDigest, RegisterWorkerSession, ServerId,
};
use crate::entities::surreal::view::AckServerConfig;
use crate::services::OrchestrationError;
use crate::services::rollout::DirtyNotifier;
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
    pub notifier: DirtyNotifier,
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
        // One transaction: rotate the key, reconcile what the worker reports, and
        // clear whatever was in flight — the worker is not running it. Refused
        // while another session still heartbeats, so a second worker pointed at
        // the same server cannot take it over just by reconnecting.
        let rotated = self
            .db
            .process(RegisterWorkerSession {
                server: server.id.clone(),
                canvas: server.canvas.clone(),
                digest: sha256_hex(&secret),
                now,
                lease_until: self.lease.until(now),
                running_revision: input.running_revision,
            })
            .await?
            .ok_or_else(|| {
                OrchestrationError::Conflict(
                    "another worker session is live for this server".into(),
                )
            })?;

        self.hub
            .supersede(&record_key(&server.id.0), rotated.refresh_key_generation);
        self.notifier.notify(&server.canvas).await;
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
        // One conditional update: an ack only lands on the revision the database
        // itself handed this session, so a stale or invented revision changes
        // nothing instead of silently marking the wrong config applied.
        let matched = self
            .db
            .process(AckServerConfig {
                server: server.id,
                canvas: server.canvas.clone(),
                revision: input.revision,
                error: input.error,
            })
            .await?;
        if !matched {
            return Err(OrchestrationError::Invalid("unknown revision".into()));
        }
        self.notifier.notify(&server.canvas).await;
        Ok(())
    }
}
