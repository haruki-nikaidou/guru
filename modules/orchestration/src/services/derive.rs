//! Config derivation: one server's `guru-worker` TOML from a canvas topology.
//!
//! Derivation is a pure function of a [`CanvasTopology`] snapshot, so the same input
//! always produces byte-identical TOML — that is what lets the rollout skip servers
//! whose config did not actually change.

use crate::entities::surreal::connection::EdgeConnectionId;
use crate::entities::surreal::node::{
    NodeId, NodeSpec, NodeWithPorts, RelayProtocol as EntityRelayProtocol,
};
use crate::entities::surreal::port::{PortDirection, PortEntity};
use crate::entities::surreal::server::{ServerId, ServerIpRecordEntity};
use crate::entities::surreal::topology::CanvasTopology;
use crate::utils::ids::record_key;
use guru_worker_config::{
    Config, Forwarding, ForwardingTo, ListenAs, LoadBalanceGroup, LogConfig, RelayHost,
    RelayProtocol, Remote,
};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error("entry {node} uses TLS, which needs certificates from a later stage")]
    TlsNotYetSupported { node: String },
    #[error("relay {node} uses protocol {protocol}, which needs the internal CA from a later stage")]
    RelayProtocolNotYetSupported {
        node: String,
        protocol: &'static str,
    },
    #[error("node {node}: canvas import/export is not supported yet")]
    UnsupportedSpec { node: String },
    #[error("pod {node} references missing ip record {ip}")]
    MissingIpRecord { node: String, ip: String },
    #[error("ip record {ip} holds '{value}', which is not an IP address")]
    InvalidIp { ip: String, value: String },
    #[error("exit {node} destination '{destination}' is not host:port")]
    InvalidDestination { node: String, destination: String },
    #[error("relay {node} listen input is not fed by a pod")]
    RelayWithoutPod { node: String },
    #[error("cycle through node {node}")]
    Cycle { node: String },
    #[error("server {server} is not part of this canvas")]
    UnknownServer { server: String },
    #[error("derived config is invalid: {0}")]
    Invalid(#[from] guru_worker_config::ConfigError),
}

#[derive(Debug, Clone)]
pub struct DerivedConfig {
    pub config: Config,
    pub toml: String,
    /// Node rows this config was derived from; they must outlive the config.
    pub nodes: Vec<NodeId>,
    /// Edge rows this config was derived from.
    pub edges: Vec<EdgeConnectionId>,
}

pub fn derive_server_config(
    topology: &CanvasTopology,
    server: &ServerId,
) -> Result<DerivedConfig, DeriveError> {
    let server_key = record_key(&server.0);
    let server_row = topology
        .servers
        .iter()
        .find(|s| record_key(&s.id.0) == server_key)
        .ok_or_else(|| DeriveError::UnknownServer {
            server: server_key.clone(),
        })?;

    let index = DeriveIndex::build(topology);
    let mut used = Used::default();
    let mut forwardings = Vec::new();

    let mut pods: Vec<&NodeWithPorts> = topology
        .nodes
        .iter()
        .filter(|n| matches!(n.node.spec, NodeSpec::Pod(_)))
        .collect();
    pods.sort_by_key(|n| record_key(&n.node.id.0));

    for pod in pods {
        let NodeSpec::Pod(cfg) = &pod.node.spec else {
            continue;
        };
        let ip = index.ip(&record_key(&cfg.ip.0)).ok_or_else(|| {
            DeriveError::MissingIpRecord {
                node: pod.node.name.clone(),
                ip: record_key(&cfg.ip.0),
            }
        })?;
        if record_key(&ip.server.0) != server_key {
            continue;
        }
        let (Some(listen_port), Some(destination_port)) = (
            index.port_by_key(pod, "listen"),
            index.port_by_key(pod, "destination"),
        ) else {
            continue;
        };
        let (Some(listen_edge), Some(destination_edge)) = (
            index.edge_on(listen_port),
            index.edge_on(destination_port),
        ) else {
            // A pod with an unconnected port is reported as a warning and skipped.
            continue;
        };

        let address: IpAddr = ip.ip.parse().map_err(|_| DeriveError::InvalidIp {
            ip: record_key(&ip.id.0),
            value: ip.ip.clone(),
        })?;

        let consumer = index
            .peer(listen_port)
            .ok_or_else(|| DeriveError::UnsupportedSpec {
                node: pod.node.name.clone(),
            })?;
        let (listen_as, receive_proxy_protocol) = match &consumer.node.spec {
            NodeSpec::Entry(entry) => {
                if entry.tls.is_some() {
                    return Err(DeriveError::TlsNotYetSupported {
                        node: consumer.node.name.clone(),
                    });
                }
                (
                    ListenAs::Raw,
                    entry.receive_proxy_protocol.map(Into::into),
                )
            }
            NodeSpec::Relay(relay) => match relay.protocol {
                // The worker auto-detects PROXY on relay ingest.
                EntityRelayProtocol::TcpRaw => (ListenAs::Relay(RelayHost::Tcp), None),
                other => {
                    return Err(DeriveError::RelayProtocolNotYetSupported {
                        node: consumer.node.name.clone(),
                        protocol: relay_protocol_name(other),
                    });
                }
            },
            _ => {
                return Err(DeriveError::UnsupportedSpec {
                    node: consumer.node.name.clone(),
                });
            }
        };
        used.node(&pod.node.id);
        used.node(&consumer.node.id);
        used.edge(&listen_edge.id);
        used.edge(&destination_edge.id);

        let producer = index
            .peer(destination_port)
            .ok_or_else(|| DeriveError::UnsupportedSpec {
                node: pod.node.name.clone(),
            })?;
        let mut visited = Vec::new();
        let to = derive_destination(&index, producer, &mut visited, &mut used)?;

        forwardings.push(Forwarding {
            tag: pod.node.name.clone(),
            listen: SocketAddr::new(address, cfg.port),
            receive_proxy_protocol,
            listen_as,
            to,
        });
    }

    let config = Config {
        ipv6_resolve: server_row.ipv6_resolve.into(),
        log: LogConfig {
            level: server_row.log_level.clone(),
        },
        forwardings,
    };
    config.validate()?;
    let toml = config.to_toml_string()?;
    Ok(DerivedConfig {
        config,
        toml,
        nodes: used.nodes(),
        edges: used.edges(),
    })
}

fn derive_destination(
    index: &DeriveIndex<'_>,
    node: &NodeWithPorts,
    visited: &mut Vec<String>,
    used: &mut Used,
) -> Result<ForwardingTo, DeriveError> {
    let key = record_key(&node.node.id.0);
    if visited.contains(&key) {
        return Err(DeriveError::Cycle {
            node: node.node.name.clone(),
        });
    }
    visited.push(key);
    used.node(&node.node.id);

    let result = match &node.node.spec {
        NodeSpec::Exit(cfg) => ForwardingTo::Exit {
            destination: Remote::parse(&cfg.destination).map_err(|_| {
                DeriveError::InvalidDestination {
                    node: node.node.name.clone(),
                    destination: cfg.destination.clone(),
                }
            })?,
            send_proxy_protocol: cfg.pass_proxy_protocol.map(Into::into),
        },
        NodeSpec::Relay(cfg) => {
            let protocol = match cfg.protocol {
                EntityRelayProtocol::TcpRaw => RelayProtocol::Tcp,
                other => {
                    return Err(DeriveError::RelayProtocolNotYetSupported {
                        node: node.node.name.clone(),
                        protocol: relay_protocol_name(other),
                    });
                }
            };
            // The relay dials the pod feeding its listen side.
            let listen_port = index.port_by_key(node, "listen");
            let listen_edge = listen_port.and_then(|p| index.edge_on(p));
            let pod = listen_port.and_then(|p| index.peer(p));
            let (Some(pod), Some(listen_edge)) = (pod, listen_edge) else {
                return Err(DeriveError::RelayWithoutPod {
                    node: node.node.name.clone(),
                });
            };
            let NodeSpec::Pod(pod_cfg) = &pod.node.spec else {
                return Err(DeriveError::RelayWithoutPod {
                    node: node.node.name.clone(),
                });
            };
            used.node(&pod.node.id);
            used.edge(&listen_edge.id);
            let pod_ip = index
                .ip(&record_key(&pod_cfg.ip.0))
                .ok_or_else(|| DeriveError::MissingIpRecord {
                    node: pod.node.name.clone(),
                    ip: record_key(&pod_cfg.ip.0),
                })?;
            let host = cfg
                .override_ip_address
                .clone()
                .unwrap_or_else(|| pod_ip.ip.clone());
            let port = cfg.override_port.unwrap_or(pod_cfg.port);
            ForwardingTo::Relay {
                protocol,
                destination: Remote::parse(&format!("{host}:{port}")).map_err(|_| {
                    DeriveError::InvalidDestination {
                        node: node.node.name.clone(),
                        destination: format!("{host}:{port}"),
                    }
                })?,
                sni: None,
            }
        }
        NodeSpec::LoadBalanceDistribute(cfg) => {
            let mut members = smallvec::SmallVec::new();
            for port in inputs_in_order(node) {
                let Some(edge) = index.edge_on(port) else {
                    continue; // unconnected members are skipped
                };
                let Some(member) = index.peer(port) else {
                    continue;
                };
                used.edge(&edge.id);
                members.push(derive_destination(index, member, visited, used)?);
            }
            ForwardingTo::LoadBalance(Box::new(LoadBalanceGroup {
                strategy: cfg.mode.into(),
                members,
            }))
        }
        NodeSpec::LoadBalanceAggregate(_) => {
            let port = inputs_in_order(node)
                .into_iter()
                .next()
                .ok_or_else(|| DeriveError::UnsupportedSpec {
                    node: node.node.name.clone(),
                })?;
            let (Some(edge), Some(source)) = (index.edge_on(port), index.peer(port)) else {
                return Err(DeriveError::UnsupportedSpec {
                    node: node.node.name.clone(),
                });
            };
            used.edge(&edge.id);
            derive_destination(index, source, visited, used)?
        }
        NodeSpec::Pod(_) | NodeSpec::Entry(_) | NodeSpec::CanvasImport(_)
        | NodeSpec::CanvasExport(_) => {
            return Err(DeriveError::UnsupportedSpec {
                node: node.node.name.clone(),
            });
        }
    };

    visited.pop();
    Ok(result)
}

fn inputs_in_order(node: &NodeWithPorts) -> Vec<&PortEntity> {
    let mut ports: Vec<&PortEntity> = node
        .ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .collect();
    ports.sort_by_key(|p| p.position);
    ports
}

fn relay_protocol_name(protocol: EntityRelayProtocol) -> &'static str {
    match protocol {
        EntityRelayProtocol::TcpRaw => "tcp_raw",
        EntityRelayProtocol::TcpTls => "tcp_tls",
        EntityRelayProtocol::Quic => "quic",
    }
}

/// The node and edge rows a derived config depends on.
#[derive(Default)]
struct Used {
    nodes: HashMap<String, NodeId>,
    edges: HashMap<String, EdgeConnectionId>,
}

impl Used {
    fn node(&mut self, id: &NodeId) {
        self.nodes.insert(record_key(&id.0), id.clone());
    }
    fn edge(&mut self, id: &EdgeConnectionId) {
        self.edges.insert(record_key(&id.0), id.clone());
    }
    fn nodes(&self) -> Vec<NodeId> {
        let mut keys: Vec<String> = self.nodes.keys().cloned().collect();
        keys.sort();
        keys.into_iter()
            .filter_map(|k| self.nodes.get(&k).cloned())
            .collect()
    }
    fn edges(&self) -> Vec<EdgeConnectionId> {
        let mut keys: Vec<String> = self.edges.keys().cloned().collect();
        keys.sort();
        keys.into_iter()
            .filter_map(|k| self.edges.get(&k).cloned())
            .collect()
    }
}

struct DeriveIndex<'a> {
    owners: HashMap<String, &'a NodeWithPorts>,
    ips: HashMap<String, &'a ServerIpRecordEntity>,
    edges_by_port: HashMap<String, &'a crate::entities::surreal::connection::EdgeConnectionEntity>,
}

impl<'a> DeriveIndex<'a> {
    fn build(topology: &'a CanvasTopology) -> Self {
        let mut owners = HashMap::new();
        for node in &topology.nodes {
            for port in &node.ports {
                owners.insert(record_key(&port.id.0), node);
            }
        }
        let mut edges_by_port = HashMap::new();
        for edge in &topology.edges {
            edges_by_port.insert(record_key(&edge.source.0), edge);
            edges_by_port.insert(record_key(&edge.target.0), edge);
        }
        Self {
            owners,
            ips: topology
                .ips
                .iter()
                .map(|ip| (record_key(&ip.id.0), ip))
                .collect(),
            edges_by_port,
        }
    }

    fn ip(&self, key: &str) -> Option<&'a ServerIpRecordEntity> {
        self.ips.get(key).copied()
    }

    fn port_by_key(&self, node: &'a NodeWithPorts, key: &str) -> Option<&'a PortEntity> {
        node.ports.iter().find(|p| p.key == key)
    }

    fn edge_on(
        &self,
        port: &PortEntity,
    ) -> Option<&'a crate::entities::surreal::connection::EdgeConnectionEntity> {
        self.edges_by_port.get(&record_key(&port.id.0)).copied()
    }

    fn peer(&self, port: &PortEntity) -> Option<&'a NodeWithPorts> {
        let edge = self.edge_on(port)?;
        let port_key = record_key(&port.id.0);
        let other = if record_key(&edge.source.0) == port_key {
            record_key(&edge.target.0)
        } else {
            record_key(&edge.source.0)
        };
        self.owners.get(&other).copied()
    }
}
