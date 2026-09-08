//! Edge operations. All edge legality lives in the topology checker.

use crate::entities::surreal::connection::{
    ConnectPorts, EdgeConnectionEntity, EdgeConnectionId, FindEdgeById, ForceDeleteEdgeRow,
    RetireEdgeRow,
};
use crate::entities::surreal::node::FindNodeById;
use crate::entities::surreal::port::{FindPortById, PortId};
use crate::entities::surreal::revision::NextRevision;
use crate::entities::surreal::topology::LoadCanvasTopology;
use crate::services::topology::{TopologyEdit, ensure_valid};
use crate::services::{OrchestrationError, rollout};
use crate::utils::ids;
use crate::utils::ids::record_key;
use auth::entities::surreal::account::AccountRole;
use auth::services::identity::Identity;
use auth::utils::rbac::Permission;
use kanau::processor::Processor;
use wakuwaku::surreal::SurrealProcessor;

#[derive(Clone)]
pub struct EdgeService {
    pub db: SurrealProcessor,
}

/// The canvas an edge endpoint belongs to.
async fn canvas_of_port(
    db: &SurrealProcessor,
    port: &PortId,
) -> Result<crate::entities::surreal::canvas::CanvasId, OrchestrationError> {
    let row = db
        .process(FindPortById { id: port.clone() })
        .await?
        .ok_or(OrchestrationError::NotFound)?;
    let node = db
        .process(FindNodeById { id: row.owner })
        .await?
        .ok_or(OrchestrationError::NotFound)?;
    Ok(node.canvas)
}

pub struct Connect {
    pub actor: Identity,
    pub output_port: PortId,
    pub input_port: PortId,
}

impl Processor<Connect> for EdgeService {
    type Output = EdgeConnectionEntity;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:Connect", skip_all, err)]
    async fn process(&self, input: Connect) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        let canvas = canvas_of_port(&self.db, &input.output_port).await?;
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: canvas.clone(),
            })
            .await?;
        ensure_valid(&topology.project(&[TopologyEdit::AddEdge {
            edge: EdgeConnectionEntity {
                id: ids::edge_id("pending-0"),
                source: input.output_port.clone(),
                target: input.input_port.clone(),
                created_rev: 0,
                retired_rev: None,
            },
        }]))?;

        let revision = self.db.process(NextRevision {}).await?;
        let edge = self
            .db
            .process(ConnectPorts {
                source: input.output_port,
                target: input.input_port,
                revision,
            })
            .await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(edge)
    }
}

pub struct Disconnect {
    pub actor: Identity,
    pub edge: EdgeConnectionId,
}

impl Processor<Disconnect> for EdgeService {
    type Output = ();
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:Disconnect", skip_all, err)]
    async fn process(&self, input: Disconnect) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        let edge = self
            .db
            .process(FindEdgeById {
                id: input.edge.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        if edge.retired_rev.is_some() {
            return Err(OrchestrationError::Conflict("edge is retired".into()));
        }
        let canvas = canvas_of_port(&self.db, &edge.source).await?;
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: canvas.clone(),
            })
            .await?;
        ensure_valid(&topology.project(&[TopologyEdit::RetireEdge {
            edge: input.edge.clone(),
        }]))?;

        let revision = self.db.process(NextRevision {}).await?;
        self.db
            .process(RetireEdgeRow {
                id: input.edge,
                revision,
            })
            .await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(())
    }
}

pub struct ForceDisconnect {
    pub actor: Identity,
    pub edge: EdgeConnectionId,
}

impl Processor<ForceDisconnect> for EdgeService {
    type Output = ();
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:ForceDisconnect", skip_all, err)]
    async fn process(&self, input: ForceDisconnect) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        if input.actor.role != AccountRole::Admin {
            return Err(OrchestrationError::PermissionDenied);
        }
        let edge = self
            .db
            .process(FindEdgeById {
                id: input.edge.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let canvas = canvas_of_port(&self.db, &edge.source).await?;
        tracing::info!(edge = %record_key(&edge.id.0), "force-deleting edge");
        self.db
            .process(ForceDeleteEdgeRow { id: input.edge })
            .await?;
        let revision = self.db.process(NextRevision {}).await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(())
    }
}
