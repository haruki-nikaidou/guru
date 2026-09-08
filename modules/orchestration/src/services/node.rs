//! Node operations. Spec changes are RCU: the old row is retired, never mutated.

use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition, FindCanvasById};
use crate::entities::surreal::connection::EdgeConnectionEntity;
use crate::entities::surreal::node::{
    CarryEdge, CreateNodeRow, FindNodeById, FindNodeWithPorts, ForceDeleteNodeRow, NewPort,
    NodeEntity, NodeId, NodeSpec, NodeWithPorts, ReplaceNodeRow, RetireNodeRow, UpdateNodeMetaRow,
};
use crate::entities::surreal::port::{PortDirection, PortEntity, PortKind};
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
pub struct NodeService {
    pub db: SurrealProcessor,
}

/// The port layout of a spec. This is the single source of truth for port keys and
/// positions; both creation and replacement generate ports from here.
pub fn port_layout(spec: &NodeSpec, item_count: u32) -> Result<Vec<NewPort>, OrchestrationError> {
    let port = |key: &str, kind: PortKind, direction: PortDirection, position: i64| NewPort {
        kind,
        direction,
        key: key.to_string(),
        position,
    };
    Ok(match spec {
        NodeSpec::Pod(_) => vec![
            port("listen", PortKind::DeriveListen, PortDirection::Output, 0),
            port(
                "destination",
                PortKind::DeriveDestination,
                PortDirection::Input,
                1,
            ),
        ],
        NodeSpec::Entry(_) => vec![port(
            "listen",
            PortKind::DeriveListen,
            PortDirection::Input,
            0,
        )],
        NodeSpec::Relay(_) => vec![
            port("listen", PortKind::DeriveListen, PortDirection::Input, 0),
            port(
                "destination",
                PortKind::DeriveDestination,
                PortDirection::Output,
                1,
            ),
        ],
        NodeSpec::Exit(_) => vec![port(
            "destination",
            PortKind::DeriveDestination,
            PortDirection::Output,
            0,
        )],
        NodeSpec::LoadBalanceDistribute(_) => {
            let count = load_balance_count(item_count)?;
            let mut ports: Vec<NewPort> = (0..count)
                .map(|i| {
                    port(
                        &format!("member_{i}"),
                        PortKind::DeriveDestination,
                        PortDirection::Input,
                        i,
                    )
                })
                .collect();
            ports.push(port(
                "destination",
                PortKind::DeriveDestination,
                PortDirection::Output,
                count,
            ));
            ports
        }
        NodeSpec::LoadBalanceAggregate(_) => {
            let count = load_balance_count(item_count)?;
            let mut ports = vec![port(
                "source",
                PortKind::DeriveDestination,
                PortDirection::Input,
                0,
            )];
            for i in 0..count {
                ports.push(port(
                    &format!("copy_{i}"),
                    PortKind::DeriveDestination,
                    PortDirection::Output,
                    i.saturating_add(1),
                ));
            }
            ports
        }
        NodeSpec::CanvasImport(_) | NodeSpec::CanvasExport(_) => {
            return Err(OrchestrationError::Invalid(
                "canvas import/export is not supported yet".into(),
            ));
        }
    })
}

fn load_balance_count(item_count: u32) -> Result<i64, OrchestrationError> {
    if item_count < 2 {
        return Err(OrchestrationError::Invalid(
            "load balance nodes need at least 2 members".into(),
        ));
    }
    Ok(i64::from(item_count))
}

/// The rows a `CreateNodeRow` will write, with placeholder ids, for validation.
fn pending_node(
    canvas: &CanvasId,
    name: &str,
    comment: &str,
    spec: &NodeSpec,
    position: CanvasUiPosition,
    ports: &[NewPort],
    revision: i64,
) -> (NodeEntity, Vec<PortEntity>) {
    let node_id = ids::node_id("pending-node");
    let node = NodeEntity {
        id: node_id.clone(),
        canvas: canvas.clone(),
        name: name.to_string(),
        comment: comment.to_string(),
        spec: spec.clone(),
        position,
        created_rev: revision,
        retired_rev: None,
        replaces: None,
    };
    let ports = ports
        .iter()
        .enumerate()
        .map(|(i, p)| PortEntity {
            id: ids::port_id(&format!("pending-port-{i}")),
            owner: node_id.clone(),
            kind: p.kind,
            direction: p.direction,
            key: p.key.clone(),
            position: p.position,
        })
        .collect();
    (node, ports)
}

pub struct CreateNode {
    pub actor: Identity,
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub position: CanvasUiPosition,
    pub item_count: u32,
}

impl Processor<CreateNode> for NodeService {
    type Output = NodeWithPorts;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:CreateNode", skip_all, err)]
    async fn process(&self, input: CreateNode) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        self.db
            .process(FindCanvasById {
                id: input.canvas.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let ports = port_layout(&input.spec, input.item_count)?;

        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: input.canvas.clone(),
            })
            .await?;
        let (node, port_rows) = pending_node(
            &input.canvas,
            &input.name,
            &input.comment,
            &input.spec,
            input.position,
            &ports,
            0,
        );
        ensure_valid(&topology.project(&[TopologyEdit::AddNode {
            node: Box::new(node),
            ports: port_rows,
        }]))?;

        let revision = self.db.process(NextRevision {}).await?;
        let created = self
            .db
            .process(CreateNodeRow {
                canvas: input.canvas.clone(),
                name: input.name,
                comment: input.comment,
                spec: input.spec,
                position: input.position,
                created_rev: revision,
                replaces: None,
                ports,
            })
            .await?;
        rollout::stamp_canvas(&self.db, &input.canvas, revision).await?;
        Ok(created)
    }
}

pub struct ReplaceNodeSpec {
    pub actor: Identity,
    pub node: NodeId,
    pub spec: NodeSpec,
    pub item_count: u32,
}

impl Processor<ReplaceNodeSpec> for NodeService {
    type Output = NodeWithPorts;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:ReplaceNodeSpec", skip_all, err)]
    async fn process(&self, input: ReplaceNodeSpec) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        let old = self
            .db
            .process(FindNodeWithPorts {
                id: input.node.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        if old.node.retired_rev.is_some() {
            return Err(OrchestrationError::Conflict("node is retired".into()));
        }
        if std::mem::discriminant(&old.node.spec) != std::mem::discriminant(&input.spec) {
            return Err(OrchestrationError::Invalid(
                "spec kind cannot change; create a new node".into(),
            ));
        }
        let ports = port_layout(&input.spec, input.item_count)?;

        let canvas = old.node.canvas.clone();
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: canvas.clone(),
            })
            .await?;

        // Edges on ports that survive the replacement are re-attached by key.
        let old_ports: Vec<&PortEntity> = old.ports.iter().collect();
        let mut carry = Vec::new();
        for edge in &topology.edges {
            for (port, is_source) in [(&edge.source, true), (&edge.target, false)] {
                let Some(old_port) = old_ports
                    .iter()
                    .find(|p| record_key(&p.id.0) == record_key(&port.0))
                else {
                    continue;
                };
                if !ports.iter().any(|p| p.key == old_port.key) {
                    return Err(OrchestrationError::Conflict(
                        "disconnect edges on removed ports first".into(),
                    ));
                }
                let other = if is_source {
                    edge.target.clone()
                } else {
                    edge.source.clone()
                };
                carry.push(CarryEdge {
                    old_edge: edge.id.clone(),
                    new_port_key: old_port.key.clone(),
                    other_port: other,
                    new_port_is_source: is_source,
                });
            }
        }

        let (node, port_rows) = pending_node(
            &canvas,
            &old.node.name,
            &old.node.comment,
            &input.spec,
            old.node.position,
            &ports,
            0,
        );
        let mut edits = vec![
            TopologyEdit::RetireNode {
                node: input.node.clone(),
            },
            TopologyEdit::AddNode {
                node: Box::new(node),
                ports: port_rows.clone(),
            },
        ];
        for (i, entry) in carry.iter().enumerate() {
            let Some(new_port) = port_rows.iter().find(|p| p.key == entry.new_port_key) else {
                continue;
            };
            let (source, target) = if entry.new_port_is_source {
                (new_port.id.clone(), entry.other_port.clone())
            } else {
                (entry.other_port.clone(), new_port.id.clone())
            };
            edits.push(TopologyEdit::AddEdge {
                edge: EdgeConnectionEntity {
                    id: ids::edge_id(&format!("pending-{i}")),
                    source,
                    target,
                    created_rev: 0,
                    retired_rev: None,
                },
            });
        }
        ensure_valid(&topology.project(&edits))?;

        let revision = self.db.process(NextRevision {}).await?;
        let replacement = self
            .db
            .process(ReplaceNodeRow {
                old: input.node,
                canvas: canvas.clone(),
                name: old.node.name,
                comment: old.node.comment,
                spec: input.spec,
                position: old.node.position,
                revision,
                ports,
                carry,
            })
            .await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(replacement)
    }
}

pub struct UpdateNodeMeta {
    pub actor: Identity,
    pub node: NodeId,
    pub name: String,
    pub comment: String,
    pub position: CanvasUiPosition,
}

impl Processor<UpdateNodeMeta> for NodeService {
    type Output = NodeEntity;
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:UpdateNodeMeta", skip_all, err)]
    async fn process(&self, input: UpdateNodeMeta) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        self.db
            .process(FindNodeById {
                id: input.node.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        // Metadata only: the derived config does not depend on any of these.
        Ok(self
            .db
            .process(UpdateNodeMetaRow {
                id: input.node,
                name: input.name,
                comment: input.comment,
                position: input.position,
            })
            .await?)
    }
}

pub struct RetireNode {
    pub actor: Identity,
    pub node: NodeId,
}

impl Processor<RetireNode> for NodeService {
    type Output = ();
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:RetireNode", skip_all, err)]
    async fn process(&self, input: RetireNode) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        let node = self
            .db
            .process(FindNodeById {
                id: input.node.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        if node.retired_rev.is_some() {
            return Err(OrchestrationError::Conflict("node is retired".into()));
        }
        let canvas = node.canvas.clone();
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: canvas.clone(),
            })
            .await?;
        ensure_valid(&topology.project(&[TopologyEdit::RetireNode {
            node: input.node.clone(),
        }]))?;

        let revision = self.db.process(NextRevision {}).await?;
        self.db
            .process(RetireNodeRow {
                id: input.node,
                revision,
            })
            .await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(())
    }
}

pub struct ForceDeleteNode {
    pub actor: Identity,
    pub node: NodeId,
}

impl Processor<ForceDeleteNode> for NodeService {
    type Output = ();
    type Error = OrchestrationError;
    #[tracing::instrument(name = "Service:ForceDeleteNode", skip_all, err)]
    async fn process(&self, input: ForceDeleteNode) -> Result<Self::Output, Self::Error> {
        input.actor.ensure(Permission::EditWorkspace)?;
        if input.actor.role != AccountRole::Admin {
            return Err(OrchestrationError::PermissionDenied);
        }
        let node = self
            .db
            .process(FindNodeById {
                id: input.node.clone(),
            })
            .await?
            .ok_or(OrchestrationError::NotFound)?;
        let canvas = node.canvas.clone();
        self.db
            .process(ForceDeleteNodeRow { id: input.node })
            .await?;
        let revision = self.db.process(NextRevision {}).await?;
        rollout::stamp_canvas(&self.db, &canvas, revision).await?;
        Ok(())
    }
}
