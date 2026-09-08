//! Re-derivation and rollout status.

use crate::entities::surreal::canvas::CanvasId;
use crate::entities::surreal::revision::{
    FindServerConfigRevision, ListRetainedRevisions, RecordServerConfigRevision,
    ServerConfigRevisionEntity,
};
use crate::entities::surreal::server::{
    FindServerById, ServerId, SetServerApplyError, SetServerDesiredRevision,
};
use crate::entities::surreal::topology::{FindCanvasOfServer, LoadCanvasTopology};
use crate::services::OrchestrationError;
use crate::services::derive::derive_server_config;
use auth::services::identity::Identity;
use auth::utils::rbac::Permission;
use chrono::{DateTime, Utc};
use kanau::processor::Processor;
use wakuwaku::surreal::SurrealProcessor;

/// Re-derives every server of a canvas after a committed change.
///
/// A server whose derived TOML is byte-identical to the text it is already rolling
/// out gets no new revision row, so unrelated servers never pin retired rows and
/// never restart their listeners. A server whose config cannot be derived records
/// the reason and keeps its current desired revision, so one unsupported node does
/// not block the rest of the canvas.
pub(crate) async fn stamp_canvas(
    db: &SurrealProcessor,
    canvas: &CanvasId,
    revision: i64,
) -> Result<(), OrchestrationError> {
    let topology = db
        .process(LoadCanvasTopology {
            canvas: canvas.clone(),
        })
        .await?;
    for server in &topology.servers {
        match derive_server_config(&topology, &server.id) {
            Ok(derived) => {
                let current = db
                    .process(FindServerConfigRevision {
                        server: server.id.clone(),
                        revision: server.desired_revision,
                    })
                    .await?;
                if current.map(|row| row.toml) == Some(derived.toml.clone()) {
                    continue;
                }
                db.process(RecordServerConfigRevision {
                    server: server.id.clone(),
                    revision,
                    nodes: derived.nodes,
                    edges: derived.edges,
                    toml: derived.toml,
                })
                .await?;
                db.process(SetServerDesiredRevision {
                    server: server.id.clone(),
                    revision,
                })
                .await?;
            }
            Err(e) => {
                tracing::warn!(server = %server.name, error = %e, "config derivation failed");
                db.process(SetServerApplyError {
                    server: server.id.clone(),
                    error: format!("derive: {e}"),
                })
                .await?;
            }
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct RolloutService {
    pub db: SurrealProcessor,
}

pub struct GetServerConfig {
    pub actor: Identity,
    pub server: ServerId,
}

impl Processor<GetServerConfig> for RolloutService {
    type Output = (i64, String);
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:GetServerConfig", skip_all, err)]
    async fn process(&self, input: GetServerConfig) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ViewWorkspace)?;
        let server = self
            .db
            .process(FindServerById {
                id: input.server.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: server.canvas.clone(),
            })
            .await?;
        let derived = derive_server_config(&topology, &server.id)?;
        Ok((server.desired_revision, derived.toml))
    }
}

#[derive(Debug, Clone)]
pub struct RolloutStatus {
    pub desired_revision: i64,
    pub applied_revision: i64,
    pub last_apply_error: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub retained: Vec<ServerConfigRevisionEntity>,
}

pub struct GetServerRolloutStatus {
    pub actor: Identity,
    pub server: ServerId,
}

impl Processor<GetServerRolloutStatus> for RolloutService {
    type Output = RolloutStatus;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:GetServerRolloutStatus", skip_all, err)]
    async fn process(&self, input: GetServerRolloutStatus) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::ViewWorkspace)?;
        let server = self
            .db
            .process(FindServerById {
                id: input.server.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let retained = self
            .db
            .process(ListRetainedRevisions {
                server: server.id.clone(),
            })
            .await?;
        Ok(RolloutStatus {
            desired_revision: server.desired_revision,
            applied_revision: server.applied_revision,
            last_apply_error: server.last_apply_error,
            last_seen_at: server.last_seen_at,
            retained,
        })
    }
}

/// The canvas a server belongs to, or [`OrchestrationError::NotFound`].
pub(crate) async fn canvas_of_server(
    db: &SurrealProcessor,
    server: &ServerId,
) -> Result<CanvasId, OrchestrationError> {
    db.process(FindCanvasOfServer {
        server: server.clone(),
    })
    .await?
    .ok_or(OrchestrationError::NotFound)
}
