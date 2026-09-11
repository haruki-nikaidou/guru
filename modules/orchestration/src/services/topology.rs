//! Topology rules.
//!
//! Every mutation runs this checker on the topology it *would* produce
//! ([`CanvasTopology::project`]) and is rejected before anything is written. The
//! same checker answers `ValidateCanvas`, where warnings are reported alongside
//! errors.
//!
//! What passes here is storable, not necessarily derivable: a half-drawn chain
//! (a relay whose `listen` side is not fed yet, a load balancer with no connected
//! members) is deliberately allowed so an operator can save mid-edit. Derivation
//! reports such a pod in `invalid_pods` and leaves every other pod on the server
//! alone — see [`crate::services::derive`].
//!
//! A rule belongs here only when the shape is unsatisfiable no matter what else
//! the operator draws; anything that a later edit can complete belongs in the
//! per-pod report instead.

use crate::entities::surreal::connection::{EdgeConnectionEntity, EdgeConnectionId};
use crate::entities::surreal::node::{NodeEntity, NodeId, NodeSpec, NodeWithPorts, RelayProtocol};
use crate::entities::surreal::port::{PortDirection, PortEntity, PortId, PortKind};
use crate::entities::surreal::server::{ServerId, ServerIpRecordEntity, ServerIpv6Resolve};
use crate::entities::surreal::topology::CanvasTopology;
use crate::utils::ids::record_key;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProblemKind {
    PortKindMismatch,
    EdgeDirectionInvalid,
    EdgeSelfNode,
    EdgeCrossCanvas,
    PortOversubscribed,
    PortShapeInvalid,
    Cycle,
    DuplicateListen,
    PodIpForeign,
    ExitDestinationInvalid,
    IpHashWithoutClientIp,
    UnsupportedSpec,
    PodPortUnconnected,
    RelaySameServer,
    DistributeSingleMember,
}

#[derive(Debug, Clone)]
pub struct TopologyProblem {
    pub severity: ProblemSeverity,
    pub kind: ProblemKind,
    pub message: String,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeConnectionId>,
    pub ports: Vec<PortId>,
}

impl TopologyProblem {
    fn error(kind: ProblemKind, message: String) -> Self {
        Self {
            severity: ProblemSeverity::Error,
            kind,
            message,
            nodes: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
        }
    }

    fn warning(kind: ProblemKind, message: String) -> Self {
        Self {
            severity: ProblemSeverity::Warning,
            kind,
            message,
            nodes: Vec::new(),
            edges: Vec::new(),
            ports: Vec::new(),
        }
    }

    fn with_nodes(mut self, nodes: Vec<NodeId>) -> Self {
        self.nodes = nodes;
        self
    }

    fn with_edges(mut self, edges: Vec<EdgeConnectionId>) -> Self {
        self.edges = edges;
        self
    }

    fn with_ports(mut self, ports: Vec<PortId>) -> Self {
        self.ports = ports;
        self
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{}", self.first.message)]
pub struct TopologyError {
    pub first: TopologyProblem,
}

/// One pending change to a topology, used to validate before writing.
#[derive(Debug, Clone)]
pub enum TopologyEdit {
    AddNode {
        node: Box<NodeEntity>,
        ports: Vec<PortEntity>,
    },
    RetireNode {
        node: NodeId,
    },
    AddEdge {
        edge: EdgeConnectionEntity,
    },
    RetireEdge {
        edge: EdgeConnectionId,
    },
    RemoveIp {
        ip: crate::entities::surreal::server::ServerIpRecordId,
    },
    SetServerSettings {
        server: ServerId,
        ipv6_resolve: ServerIpv6Resolve,
        log_level: String,
    },
}

impl CanvasTopology {
    /// The topology that results from applying `edits`, without writing anything.
    pub fn project(&self, edits: &[TopologyEdit]) -> CanvasTopology {
        let mut out = self.clone();
        for edit in edits {
            match edit {
                TopologyEdit::AddNode { node, ports } => out.nodes.push(NodeWithPorts {
                    node: (**node).clone(),
                    ports: ports.clone(),
                }),
                TopologyEdit::RetireNode { node } => {
                    let key = record_key(&node.0);
                    let ports: HashSet<String> = out
                        .nodes
                        .iter()
                        .filter(|n| record_key(&n.node.id.0) == key)
                        .flat_map(|n| n.ports.iter().map(|p| record_key(&p.id.0)))
                        .collect();
                    out.nodes.retain(|n| record_key(&n.node.id.0) != key);
                    out.edges.retain(|e| {
                        !ports.contains(&record_key(&e.source.0))
                            && !ports.contains(&record_key(&e.target.0))
                    });
                }
                TopologyEdit::AddEdge { edge } => out.edges.push(edge.clone()),
                TopologyEdit::RetireEdge { edge } => {
                    let key = record_key(&edge.0);
                    out.edges.retain(|e| record_key(&e.id.0) != key);
                }
                TopologyEdit::RemoveIp { ip } => {
                    let key = record_key(&ip.0);
                    out.ips.retain(|row| record_key(&row.id.0) != key);
                }
                TopologyEdit::SetServerSettings {
                    server,
                    ipv6_resolve,
                    log_level,
                } => {
                    let key = record_key(&server.0);
                    for s in out.servers.iter_mut() {
                        if record_key(&s.id.0) == key {
                            s.ipv6_resolve = *ipv6_resolve;
                            s.log_level = log_level.clone();
                        }
                    }
                }
            }
        }
        out
    }
}

/// Errors first, then warnings.
pub fn analyze(topology: &CanvasTopology) -> Vec<TopologyProblem> {
    let index = Index::build(topology);
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    check_edges(&index, topology, &mut errors);
    check_port_shapes(&index, &mut errors);
    check_specs(&index, &mut errors);
    check_duplicate_listen(&index, &mut errors);
    check_cycles(&index, &mut errors);
    check_ip_hash(&index, &mut errors);
    check_warnings(&index, &mut warnings);

    errors.append(&mut warnings);
    errors
}

/// The error is boxed: it carries the offending problem with its node, edge, and
/// port lists, which is far larger than the success path.
pub fn ensure_valid(topology: &CanvasTopology) -> Result<(), Box<TopologyError>> {
    match analyze(topology)
        .into_iter()
        .find(|p| p.severity == ProblemSeverity::Error)
    {
        Some(first) => Err(Box::new(TopologyError { first })),
        None => Ok(()),
    }
}

/// Lookup tables over one topology snapshot, keyed by record key.
struct Index<'a> {
    canvas: String,
    nodes: HashMap<String, &'a NodeWithPorts>,
    /// port key -> (port, owning node)
    ports: HashMap<String, (&'a PortEntity, &'a NodeWithPorts)>,
    ips: HashMap<String, &'a ServerIpRecordEntity>,
    servers: HashSet<String>,
    /// port key -> live edges touching it
    edges_by_port: HashMap<String, Vec<&'a EdgeConnectionEntity>>,
}

impl<'a> Index<'a> {
    fn build(topology: &'a CanvasTopology) -> Self {
        let mut nodes = HashMap::new();
        let mut ports = HashMap::new();
        for node in &topology.nodes {
            nodes.insert(record_key(&node.node.id.0), node);
            for port in &node.ports {
                ports.insert(record_key(&port.id.0), (port, node));
            }
        }
        let mut edges_by_port: HashMap<String, Vec<&EdgeConnectionEntity>> = HashMap::new();
        for edge in &topology.edges {
            edges_by_port
                .entry(record_key(&edge.source.0))
                .or_default()
                .push(edge);
            edges_by_port
                .entry(record_key(&edge.target.0))
                .or_default()
                .push(edge);
        }
        Self {
            canvas: record_key(&topology.canvas.0),
            nodes,
            ports,
            ips: topology
                .ips
                .iter()
                .map(|ip| (record_key(&ip.id.0), ip))
                .collect(),
            servers: topology
                .servers
                .iter()
                .map(|s| record_key(&s.id.0))
                .collect(),
            edges_by_port,
        }
    }

    fn port(&self, id: &PortId) -> Option<(&'a PortEntity, &'a NodeWithPorts)> {
        self.ports.get(&record_key(&id.0)).copied()
    }

    /// The single live edge on a port, if any.
    fn edge_on(&self, port: &PortEntity) -> Option<&'a EdgeConnectionEntity> {
        self.edges_by_port
            .get(&record_key(&port.id.0))
            .and_then(|edges| edges.first().copied())
    }

    /// The node on the other side of a port's edge.
    fn peer(&self, port: &PortEntity) -> Option<&'a NodeWithPorts> {
        let edge = self.edge_on(port)?;
        let other = if record_key(&edge.source.0) == record_key(&port.id.0) {
            &edge.target
        } else {
            &edge.source
        };
        self.port(other).map(|(_, node)| node)
    }

    fn port_by_key(&self, node: &'a NodeWithPorts, key: &str) -> Option<&'a PortEntity> {
        node.ports.iter().find(|p| p.key == key)
    }
}

fn check_edges(index: &Index<'_>, topology: &CanvasTopology, out: &mut Vec<TopologyProblem>) {
    let mut usage: HashMap<String, (usize, PortId)> = HashMap::new();
    for edge in &topology.edges {
        let edge_key = record_key(&edge.id.0);
        for endpoint in [&edge.source, &edge.target] {
            let entry = usage
                .entry(record_key(&endpoint.0))
                .or_insert((0, endpoint.clone()));
            entry.0 = entry.0.saturating_add(1);
        }

        let (Some((source, source_node)), Some((target, target_node))) =
            (index.port(&edge.source), index.port(&edge.target))
        else {
            out.push(
                TopologyProblem::error(
                    ProblemKind::EdgeDirectionInvalid,
                    format!("edge {edge_key} must connect an output port to an input port"),
                )
                .with_edges(vec![edge.id.clone()]),
            );
            continue;
        };
        if source.direction != PortDirection::Output || target.direction != PortDirection::Input {
            out.push(
                TopologyProblem::error(
                    ProblemKind::EdgeDirectionInvalid,
                    format!("edge {edge_key} must connect an output port to an input port"),
                )
                .with_edges(vec![edge.id.clone()])
                .with_ports(vec![edge.source.clone(), edge.target.clone()]),
            );
            continue;
        }
        if source.kind != target.kind {
            out.push(
                TopologyProblem::error(
                    ProblemKind::PortKindMismatch,
                    format!(
                        "edge {edge_key} connects {} to {}",
                        kind_name(source.kind),
                        kind_name(target.kind)
                    ),
                )
                .with_edges(vec![edge.id.clone()])
                .with_ports(vec![edge.source.clone(), edge.target.clone()]),
            );
        }
        if record_key(&source_node.node.id.0) == record_key(&target_node.node.id.0) {
            out.push(
                TopologyProblem::error(
                    ProblemKind::EdgeSelfNode,
                    format!(
                        "edge {edge_key} connects node {} to itself",
                        source_node.node.name
                    ),
                )
                .with_edges(vec![edge.id.clone()])
                .with_nodes(vec![source_node.node.id.clone()]),
            );
        }
        if record_key(&source_node.node.canvas.0) != index.canvas
            || record_key(&target_node.node.canvas.0) != index.canvas
        {
            out.push(
                TopologyProblem::error(
                    ProblemKind::EdgeCrossCanvas,
                    format!("edge {edge_key} crosses canvas boundaries"),
                )
                .with_edges(vec![edge.id.clone()]),
            );
        }
    }

    let mut oversubscribed: Vec<_> = usage
        .into_iter()
        .filter(|(_, (count, _))| *count > 1)
        .collect();
    oversubscribed.sort_by(|a, b| a.0.cmp(&b.0));
    for (port_key, (count, port_id)) in oversubscribed {
        let name = index
            .ports
            .get(&port_key)
            .map(|(_, node)| node.node.name.clone())
            .unwrap_or_default();
        out.push(
            TopologyProblem::error(
                ProblemKind::PortOversubscribed,
                format!("port {port_key} of node {name} carries {count} edges"),
            )
            .with_ports(vec![port_id]),
        );
    }
}

/// `(kind, direction, exact count or "at least 2")` a spec's ports must match.
fn expected_ports(spec: &NodeSpec) -> Option<Vec<(PortKind, PortDirection, Multiplicity)>> {
    use Multiplicity::{AtLeastTwo, One};
    use PortDirection::{Input, Output};
    use PortKind::{DeriveDestination, DeriveListen};
    Some(match spec {
        NodeSpec::Pod(_) => vec![(DeriveListen, Output, One), (DeriveDestination, Input, One)],
        NodeSpec::Entry(_) => vec![(DeriveListen, Input, One)],
        NodeSpec::Relay(_) => vec![(DeriveListen, Input, One), (DeriveDestination, Output, One)],
        NodeSpec::Exit(_) => vec![(DeriveDestination, Output, One)],
        NodeSpec::LoadBalanceDistribute(_) => vec![
            (DeriveDestination, Input, AtLeastTwo),
            (DeriveDestination, Output, One),
        ],
        NodeSpec::LoadBalanceAggregate(_) => vec![
            (DeriveDestination, Input, One),
            (DeriveDestination, Output, AtLeastTwo),
        ],
        NodeSpec::CanvasImport(_) | NodeSpec::CanvasExport(_) => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Multiplicity {
    One,
    AtLeastTwo,
}

fn check_port_shapes(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    for node in sorted_nodes(index) {
        let Some(expected) = expected_ports(&node.node.spec) else {
            continue; // unsupported specs are reported separately
        };
        let mut counts: HashMap<(PortKind, PortDirection), usize> = HashMap::new();
        for port in &node.ports {
            let slot = counts.entry((port.kind, port.direction)).or_insert(0);
            *slot = slot.saturating_add(1);
        }
        let mut ok = counts.len() == expected.len();
        for (kind, direction, multiplicity) in expected {
            let count = counts.get(&(kind, direction)).copied().unwrap_or(0);
            ok &= match multiplicity {
                Multiplicity::One => count == 1,
                Multiplicity::AtLeastTwo => count >= 2,
            };
        }
        if !ok {
            out.push(
                TopologyProblem::error(
                    ProblemKind::PortShapeInvalid,
                    format!("node {} has invalid ports for its spec", node.node.name),
                )
                .with_nodes(vec![node.node.id.clone()]),
            );
        }
    }
}

fn check_specs(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    for node in sorted_nodes(index) {
        match &node.node.spec {
            NodeSpec::CanvasImport(_) | NodeSpec::CanvasExport(_) => out.push(
                TopologyProblem::error(
                    ProblemKind::UnsupportedSpec,
                    format!(
                        "node {}: canvas import/export is not supported until subcanvases land",
                        node.node.name
                    ),
                )
                .with_nodes(vec![node.node.id.clone()]),
            ),
            NodeSpec::Pod(cfg) => {
                let foreign = match index.ips.get(&record_key(&cfg.ip.0)) {
                    None => true,
                    Some(ip) => !index.servers.contains(&record_key(&ip.server.0)),
                };
                if foreign {
                    out.push(
                        TopologyProblem::error(
                            ProblemKind::PodIpForeign,
                            format!(
                                "pod {} references an ip record outside this canvas",
                                node.node.name
                            ),
                        )
                        .with_nodes(vec![node.node.id.clone()]),
                    );
                }
            }
            // An exit whose destination is not filled in yet is the half-drawn case
            // this module documents: a later edit completes it, and derivation
            // already refuses to publish the pod behind it (`invalid_pods`). Only a
            // destination that was actually written and cannot ever parse is an
            // error.
            NodeSpec::Exit(cfg) if cfg.destination.is_empty() => {
                out.push(
                    TopologyProblem::warning(
                        ProblemKind::ExitDestinationInvalid,
                        format!("exit {} has no destination yet", node.node.name),
                    )
                    .with_nodes(vec![node.node.id.clone()]),
                );
            }
            NodeSpec::Exit(cfg) if guru_worker_config::Remote::parse(&cfg.destination).is_err() => {
                out.push(
                    TopologyProblem::error(
                        ProblemKind::ExitDestinationInvalid,
                        format!(
                            "exit {} destination '{}' is not host:port",
                            node.node.name, cfg.destination
                        ),
                    )
                    .with_nodes(vec![node.node.id.clone()]),
                );
            }
            _ => {}
        }
    }
}

/// QUIC iff the pod's listen output is consumed by a QUIC relay.
fn pod_transport(index: &Index<'_>, pod: &NodeWithPorts) -> &'static str {
    let quic = index
        .port_by_key(pod, "listen")
        .and_then(|port| index.peer(port))
        .map(|peer| {
            matches!(&peer.node.spec, NodeSpec::Relay(cfg) if cfg.protocol == RelayProtocol::Quic)
        })
        .unwrap_or(false);
    if quic { "quic" } else { "tcp" }
}

fn check_duplicate_listen(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    let mut seen: HashMap<(String, u16, &'static str), &NodeWithPorts> = HashMap::new();
    for node in sorted_nodes(index) {
        let NodeSpec::Pod(cfg) = &node.node.spec else {
            continue;
        };
        let transport = pod_transport(index, node);
        let key = (record_key(&cfg.ip.0), cfg.port, transport);
        match seen.get(&key) {
            Some(first) => {
                let ip = index
                    .ips
                    .get(&key.0)
                    .map(|ip| ip.ip.clone())
                    .unwrap_or_else(|| key.0.clone());
                out.push(
                    TopologyProblem::error(
                        ProblemKind::DuplicateListen,
                        format!(
                            "pods {} and {} both listen on {}:{} ({})",
                            first.node.name, node.node.name, ip, cfg.port, transport
                        ),
                    )
                    .with_nodes(vec![first.node.id.clone(), node.node.id.clone()]),
                );
            }
            None => {
                seen.insert(key, node);
            }
        }
    }
}

/// `consumer -> producers`: a node depends on whatever feeds its inputs, and a relay
/// additionally depends on the pod it dials into.
fn dependency_graph(index: &Index<'_>) -> HashMap<String, Vec<String>> {
    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for node in index.nodes.values() {
        let key = record_key(&node.node.id.0);
        let deps = graph.entry(key).or_default();
        for port in node
            .ports
            .iter()
            .filter(|p| p.direction == PortDirection::Input)
        {
            if let Some(peer) = index.peer(port) {
                deps.push(record_key(&peer.node.id.0));
            }
        }
    }
    for node in index.nodes.values() {
        if !matches!(node.node.spec, NodeSpec::Relay(_)) {
            continue;
        }
        // The relay's listen input is fed by the pod it dials; that pod's destination
        // subtree continues this traffic path.
        if let Some(listen) = index.port_by_key(node, "listen")
            && let Some(pod) = index.peer(listen)
        {
            graph
                .entry(record_key(&node.node.id.0))
                .or_default()
                .push(record_key(&pod.node.id.0));
        }
    }
    graph
}

fn check_cycles(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    let graph = dependency_graph(index);
    let mut colour: HashMap<&str, u8> = HashMap::new(); // 0 = open, 1 = done
    let mut reported: HashSet<String> = HashSet::new();
    let mut keys: Vec<&String> = graph.keys().collect();
    keys.sort();

    for start in keys {
        if colour.get(start.as_str()).copied().unwrap_or(0) == 1 {
            continue;
        }
        // Iterative DFS carrying the current path, so a back edge names its cycle.
        let mut stack: Vec<(&str, usize)> = vec![(start.as_str(), 0)];
        let mut path: Vec<&str> = vec![start.as_str()];
        let mut on_path: HashSet<&str> = HashSet::from([start.as_str()]);
        while let Some((node, edge_index)) = stack.pop() {
            let deps = graph.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if edge_index >= deps.len() {
                colour.insert(node, 1);
                path.pop();
                on_path.remove(node);
                continue;
            }
            stack.push((node, edge_index.saturating_add(1)));
            let next = deps[edge_index].as_str();
            if on_path.contains(next) {
                let start_at = path.iter().position(|n| *n == next).unwrap_or(0);
                let mut cycle: Vec<String> =
                    path[start_at..].iter().map(|n| (*n).to_string()).collect();
                cycle.sort();
                let signature = cycle.join(",");
                if reported.insert(signature) {
                    let named = index.nodes.get(&cycle[0]).map(|n| n.node.name.clone());
                    out.push(
                        TopologyProblem::error(
                            ProblemKind::Cycle,
                            format!("cycle through node {}", named.unwrap_or_default()),
                        )
                        .with_nodes(
                            cycle
                                .iter()
                                .filter_map(|k| index.nodes.get(k))
                                .map(|n| n.node.id.clone())
                                .collect(),
                        ),
                    );
                }
                continue;
            }
            if colour.get(next).copied().unwrap_or(0) == 1 {
                continue;
            }
            stack.push((next, 0));
            path.push(next);
            on_path.insert(next);
        }
    }
}

/// Whether the destination subtree behind this node hashes on the client address.
fn iphash_reachable(index: &Index<'_>, node: &NodeWithPorts, depth: usize) -> bool {
    if depth > 64 {
        return false; // a cycle is reported separately
    }
    match &node.node.spec {
        NodeSpec::LoadBalanceDistribute(cfg) => {
            if cfg.mode == crate::entities::surreal::node::LoadBalanceMode::IpHash {
                return true;
            }
            node.ports
                .iter()
                .filter(|p| p.direction == PortDirection::Input)
                .filter_map(|p| index.peer(p))
                .any(|member| iphash_reachable(index, member, depth.saturating_add(1)))
        }
        NodeSpec::LoadBalanceAggregate(_) => node
            .ports
            .iter()
            .find(|p| p.direction == PortDirection::Input)
            .and_then(|p| index.peer(p))
            .map(|source| iphash_reachable(index, source, depth.saturating_add(1)))
            .unwrap_or(false),
        _ => false,
    }
}

fn check_ip_hash(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    for pod in sorted_nodes(index) {
        if !matches!(pod.node.spec, NodeSpec::Pod(_)) {
            continue;
        }
        let listen_consumer = index.port_by_key(pod, "listen").and_then(|p| index.peer(p));
        let client_ip_known = match listen_consumer.map(|c| &c.node.spec) {
            Some(NodeSpec::Entry(cfg)) => cfg.receive_proxy_protocol.is_some(),
            // A relay hop always carries PROXY v2, and the relay itself is exempt.
            Some(NodeSpec::Relay(_)) => true,
            _ => true,
        };
        if client_ip_known {
            continue;
        }
        let uses_hash = index
            .port_by_key(pod, "destination")
            .and_then(|p| index.peer(p))
            .map(|producer| iphash_reachable(index, producer, 0))
            .unwrap_or(false);
        if uses_hash {
            out.push(
                TopologyProblem::error(
                    ProblemKind::IpHashWithoutClientIp,
                    format!(
                        "load balancer under pod {} uses ip_hash but the client address is unknown",
                        pod.node.name
                    ),
                )
                .with_nodes(vec![pod.node.id.clone()]),
            );
        }
    }
}

fn check_warnings(index: &Index<'_>, out: &mut Vec<TopologyProblem>) {
    for node in sorted_nodes(index) {
        match &node.node.spec {
            NodeSpec::Pod(_) => {
                if node.ports.iter().any(|p| index.edge_on(p).is_none()) {
                    out.push(
                        TopologyProblem::warning(
                            ProblemKind::PodPortUnconnected,
                            format!(
                                "pod {} has an unconnected port; it will be skipped",
                                node.node.name
                            ),
                        )
                        .with_nodes(vec![node.node.id.clone()]),
                    );
                }
            }
            NodeSpec::Relay(_) => {
                let listen_pod = index
                    .port_by_key(node, "listen")
                    .and_then(|p| index.peer(p));
                let target_pod = index
                    .port_by_key(node, "destination")
                    .and_then(|p| index.peer(p));
                if let (Some(a), Some(b)) = (listen_pod, target_pod)
                    && let (Some(sa), Some(sb)) = (pod_server(index, a), pod_server(index, b))
                    && sa == sb
                {
                    out.push(
                        TopologyProblem::warning(
                            ProblemKind::RelaySameServer,
                            format!("relay {} hops within one server", node.node.name),
                        )
                        .with_nodes(vec![node.node.id.clone()]),
                    );
                }
            }
            NodeSpec::LoadBalanceDistribute(_) => {
                let connected = node
                    .ports
                    .iter()
                    .filter(|p| p.direction == PortDirection::Input)
                    .filter(|p| index.edge_on(p).is_some())
                    .count();
                if connected == 1 {
                    out.push(
                        TopologyProblem::warning(
                            ProblemKind::DistributeSingleMember,
                            format!("load balancer {} has a single member", node.node.name),
                        )
                        .with_nodes(vec![node.node.id.clone()]),
                    );
                }
            }
            _ => {}
        }
    }
}

/// The server key a pod listens on.
fn pod_server(index: &Index<'_>, pod: &NodeWithPorts) -> Option<String> {
    let NodeSpec::Pod(cfg) = &pod.node.spec else {
        return None;
    };
    index
        .ips
        .get(&record_key(&cfg.ip.0))
        .map(|ip| record_key(&ip.server.0))
}

/// Nodes in record-key order, so problems are reported deterministically.
fn sorted_nodes<'a>(index: &Index<'a>) -> Vec<&'a NodeWithPorts> {
    let mut keys: Vec<&String> = index.nodes.keys().collect();
    keys.sort();
    keys.into_iter()
        .filter_map(|k| index.nodes.get(k))
        .copied()
        .collect()
}

fn kind_name(kind: PortKind) -> &'static str {
    match kind {
        PortKind::DeriveListen => "derive_listen",
        PortKind::DeriveDestination => "derive_destination",
    }
}
