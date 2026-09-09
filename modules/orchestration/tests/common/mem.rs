#![allow(dead_code)]

//! In-memory [`CanvasTopology`] builder for the pure topology/derive tests.

use orchestration::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use orchestration::entities::surreal::connection::{EdgeConnectionEntity, EdgeConnectionId};
use orchestration::entities::surreal::node::{NodeEntity, NodeId, NodeSpec, NodeWithPorts};
use orchestration::entities::surreal::port::{PortDirection, PortEntity, PortId, PortKind};
use orchestration::entities::surreal::server::{
    ServerEntity, ServerId, ServerIpRecordEntity, ServerIpRecordId, ServerIpv6Resolve,
};
use orchestration::entities::surreal::topology::CanvasTopology;
use orchestration::utils::ids;

pub struct Builder {
    canvas: CanvasId,
    servers: Vec<ServerEntity>,
    ips: Vec<ServerIpRecordEntity>,
    nodes: Vec<NodeWithPorts>,
    edges: Vec<EdgeConnectionEntity>,
}

/// `(port key, kind, direction, position)`
pub type PortSpec = (String, PortKind, PortDirection, i64);

fn spec(key: &str, kind: PortKind, direction: PortDirection, position: i64) -> PortSpec {
    (key.to_string(), kind, direction, position)
}

pub fn pod_ports() -> Vec<PortSpec> {
    vec![
        spec("listen", PortKind::DeriveListen, PortDirection::Output, 0),
        spec(
            "destination",
            PortKind::DeriveDestination,
            PortDirection::Input,
            1,
        ),
    ]
}

pub fn entry_ports() -> Vec<PortSpec> {
    vec![spec(
        "listen",
        PortKind::DeriveListen,
        PortDirection::Input,
        0,
    )]
}

pub fn relay_ports() -> Vec<PortSpec> {
    vec![
        spec("listen", PortKind::DeriveListen, PortDirection::Input, 0),
        spec(
            "destination",
            PortKind::DeriveDestination,
            PortDirection::Output,
            1,
        ),
    ]
}

pub fn exit_ports() -> Vec<PortSpec> {
    vec![spec(
        "destination",
        PortKind::DeriveDestination,
        PortDirection::Output,
        0,
    )]
}

pub fn distribute_ports(members: i64) -> Vec<PortSpec> {
    let mut ports: Vec<PortSpec> = (0..members)
        .map(|i| {
            spec(
                &format!("member_{i}"),
                PortKind::DeriveDestination,
                PortDirection::Input,
                i,
            )
        })
        .collect();
    ports.push(spec(
        "destination",
        PortKind::DeriveDestination,
        PortDirection::Output,
        members,
    ));
    ports
}

pub fn aggregate_ports(copies: i64) -> Vec<PortSpec> {
    let mut ports: Vec<PortSpec> = vec![spec(
        "source",
        PortKind::DeriveDestination,
        PortDirection::Input,
        0,
    )];
    for i in 0..copies {
        ports.push(spec(
            &format!("copy_{i}"),
            PortKind::DeriveDestination,
            PortDirection::Output,
            i + 1,
        ));
    }
    ports
}

impl Builder {
    pub fn new(canvas: &str) -> Self {
        Self {
            canvas: ids::canvas_id(canvas),
            servers: Vec::new(),
            ips: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn server(&mut self, key: &str) -> ServerId {
        let id = ids::server_id(key);
        self.servers.push(ServerEntity {
            id: id.clone(),
            canvas: self.canvas.clone(),
            name: key.to_string(),
            icon: String::new(),
            comment: String::new(),
            position: CanvasUiPosition { x: 0, y: 0 },
            ipv6_resolve: ServerIpv6Resolve::Tolerated,
            log_level: "info".to_string(),
            desired_revision: 0,
            applied_revision: 0,
            last_apply_error: None,
            current_dynamic_refresh_key: None,
            refresh_key_generation: 0,
            watch_epoch: 0,
            session_lease_until: None,
            last_seen_at: None,
        });
        id
    }

    pub fn ip(&mut self, key: &str, server: &ServerId, ip: &str) -> ServerIpRecordId {
        let id = ids::server_ip_id(key);
        self.ips.push(ServerIpRecordEntity {
            id: id.clone(),
            server: server.clone(),
            ip: ip.to_string(),
            country: "jp".to_string(),
        });
        id
    }

    pub fn node(&mut self, key: &str, spec: NodeSpec, ports: Vec<PortSpec>) -> NodeId {
        self.named_node(key, key, spec, ports)
    }

    pub fn named_node(
        &mut self,
        key: &str,
        name: &str,
        spec: NodeSpec,
        ports: Vec<PortSpec>,
    ) -> NodeId {
        let id = ids::node_id(key);
        let ports = ports
            .into_iter()
            .map(|(port_key, kind, direction, position)| PortEntity {
                id: ids::port_id(&format!("{key}-{port_key}")),
                owner: id.clone(),
                kind,
                direction,
                key: port_key,
                position,
            })
            .collect();
        self.nodes.push(NodeWithPorts {
            node: NodeEntity {
                id: id.clone(),
                canvas: self.canvas.clone(),
                name: name.to_string(),
                comment: String::new(),
                spec,
                position: CanvasUiPosition { x: 0, y: 0 },
                created_rev: 1,
                retired_rev: None,
                replaces: None,
            },
            ports,
        });
        id
    }

    /// Connects an output port to an input port, both named `<node key>-<port key>`.
    pub fn connect(&mut self, source: &str, target: &str) -> EdgeConnectionId {
        let id = ids::edge_id(&format!("{source}->{target}"));
        self.edges.push(EdgeConnectionEntity {
            id: id.clone(),
            source: ids::port_id(source),
            target: ids::port_id(target),
            created_rev: 1,
            retired_rev: None,
        });
        id
    }

    /// Connects two ports verbatim, for the direction/kind violation tests.
    pub fn connect_raw(&mut self, source: PortId, target: PortId) -> EdgeConnectionId {
        let id = ids::edge_id(&format!(
            "{}->{}",
            ids::record_key(&source.0),
            ids::record_key(&target.0)
        ));
        self.edges.push(EdgeConnectionEntity {
            id: id.clone(),
            source,
            target,
            created_rev: 1,
            retired_rev: None,
        });
        id
    }

    pub fn canvas_id(&self) -> CanvasId {
        self.canvas.clone()
    }

    pub fn build(&self) -> CanvasTopology {
        CanvasTopology {
            canvas: self.canvas.clone(),
            servers: self.servers.clone(),
            ips: self.ips.clone(),
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        }
    }
}

/// The port id the builder assigns to `<node key>-<port key>`.
pub fn port(node_key: &str, port_key: &str) -> PortId {
    ids::port_id(&format!("{node_key}-{port_key}"))
}
