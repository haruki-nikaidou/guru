//! Node operations. Specs and ports are edited in place; ports keep their identity
//! across a spec change, so the edges attached to them survive it.

use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition, FindCanvasById};
use crate::entities::surreal::node::{
    CreateNodeRow, DeleteNodeRow, FindNodeById, FindNodeWithPorts, NewPort, NodeEntity, NodeId,
    NodeSpec, NodeWithPorts, UpdateNodeMetaRow, UpdateNodeSpecRow,
};
use crate::entities::surreal::port::{PortDirection, PortEntity, PortKind};
use crate::entities::surreal::topology::LoadCanvasTopology;
use crate::entities::surreal::view::ListServerConfigViewsByCanvas;
use crate::services::OrchestrationError;
use crate::services::converge::ensure_switch_safe;
use crate::services::rollout::DirtyNotifier;
use crate::services::topology::{TopologyEdit, ensure_valid};
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
    pub notifier: DirtyNotifier,
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
) -> (NodeEntity, Vec<PortEntity>) {
    let node_id = ids::node_id("pending-node");
    let node = NodeEntity {
        id: node_id.clone(),
        canvas: canvas.clone(),
        name: name.to_string(),
        comment: comment.to_string(),
        spec: spec.clone(),
        position,
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
        );
        let projected = topology.project(&[TopologyEdit::AddNode {
            node: Box::new(node),
            ports: port_rows,
        }]);
        ensure_valid(&projected)?;
        let views = self
            .db
            .process(ListServerConfigViewsByCanvas {
                canvas: input.canvas.clone(),
            })
            .await?;
        ensure_switch_safe(&projected, &views)?;

        let created = self
            .db
            .process(CreateNodeRow {
                canvas: input.canvas.clone(),
                name: input.name,
                comment: input.comment,
                spec: input.spec,
                position: input.position,
                ports,
            })
            .await?;
        self.notifier.notify(&input.canvas).await;
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

        // A port whose key survives keeps its row, and with it every edge attached
        // to it. The projection mirrors that: kept keys reuse the existing port id,
        // so the edges below re-attach to exactly the rows the write will keep.
        let mut port_rows = Vec::with_capacity(ports.len());
        for (i, port) in ports.iter().enumerate() {
            let id = match old.ports.iter().find(|p| p.key == port.key) {
                Some(existing) => existing.id.clone(),
                None => ids::port_id(&format!("pending-port-{i}")),
            };
            port_rows.push(PortEntity {
                id,
                owner: old.node.id.clone(),
                kind: port.kind,
                direction: port.direction,
                key: port.key.clone(),
                position: port.position,
            });
        }
        let kept: Vec<String> = port_rows.iter().map(|p| record_key(&p.id.0)).collect();
        let mut carried = Vec::new();
        for edge in &topology.edges {
            for port in [&edge.source, &edge.target] {
                let Some(old_port) = old
                    .ports
                    .iter()
                    .find(|p| record_key(&p.id.0) == record_key(&port.0))
                else {
                    continue;
                };
                if !kept.contains(&record_key(&old_port.id.0)) {
                    return Err(OrchestrationError::Conflict(
                        "disconnect edges on removed ports first".into(),
                    ));
                }
                carried.push(edge.clone());
            }
        }

        let node = NodeEntity {
            id: old.node.id.clone(),
            canvas: canvas.clone(),
            name: old.node.name.clone(),
            comment: old.node.comment.clone(),
            spec: input.spec.clone(),
            position: old.node.position,
        };
        let mut edits = vec![
            TopologyEdit::RetireNode {
                node: input.node.clone(),
            },
            TopologyEdit::AddNode {
                node: Box::new(node),
                ports: port_rows,
            },
        ];
        for edge in carried {
            edits.push(TopologyEdit::AddEdge { edge });
        }
        let projected = topology.project(&edits);
        ensure_valid(&projected)?;
        let views = self
            .db
            .process(ListServerConfigViewsByCanvas {
                canvas: canvas.clone(),
            })
            .await?;
        ensure_switch_safe(&projected, &views)?;

        let updated = self
            .db
            .process(UpdateNodeSpecRow {
                id: input.node,
                canvas: canvas.clone(),
                spec: input.spec,
                ports,
            })
            .await?;
        self.notifier.notify(&canvas).await;
        Ok(updated)
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

/// Deletes a node after checking the canvas still validates without it.
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
        let canvas = node.canvas.clone();
        let topology = self
            .db
            .process(LoadCanvasTopology {
                canvas: canvas.clone(),
            })
            .await?;
        let projected = topology.project(&[TopologyEdit::RetireNode {
            node: input.node.clone(),
        }]);
        ensure_valid(&projected)?;
        let views = self
            .db
            .process(ListServerConfigViewsByCanvas {
                canvas: canvas.clone(),
            })
            .await?;
        ensure_switch_safe(&projected, &views)?;

        self.db
            .process(DeleteNodeRow {
                id: input.node,
                canvas: canvas.clone(),
            })
            .await?;
        self.notifier.notify(&canvas).await;
        Ok(())
    }
}

/// Deletes a node without validating the canvas it leaves behind. Admin only.
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
            .process(DeleteNodeRow {
                id: input.node,
                canvas: canvas.clone(),
            })
            .await?;
        self.notifier.notify(&canvas).await;
        Ok(())
    }
}
