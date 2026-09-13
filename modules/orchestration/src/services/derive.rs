//! Config derivation: one server's ideal `guru-worker` config from a canvas tree.
//!
//! Derivation is a pure function of a [`CanvasTopology`] snapshot, so the same input
//! always produces byte-identical output — that is what lets a derivation pass skip
//! servers whose config did not actually change. What a server may *safely* run
//! right now is decided afterwards, in [`crate::services::converge`].
//!
//! The snapshot is a whole canvas tree and every walk goes through
//! [`Index::peer`], which resolves import/export boundaries, so a nested graph
//! derives exactly the TOML its flattened equivalent would.
//!
//! Failure is per pod, not per server: each pod's listen and destination walks are
//! caught at the pod, so one malformed chain costs exactly its own forwarding and
//! every healthy pod on the server still rolls out. Only a cross-pod failure — two
//! pods claiming one socket — can fail the whole server.

use crate::entities::surreal::node::{
    NodeSpec, NodeWithPorts, PodConfig, RelayProtocol as EntityRelayProtocol,
};
use crate::entities::surreal::port::{PortDirection, PortEntity};
use crate::entities::surreal::server::{ServerId, ServerIpRecordEntity};
use crate::entities::surreal::topology::CanvasTopology;
use crate::entities::surreal::view::{ForwardingDeps, InvalidPod, ListenProtocol, ListenerCap};
use crate::services::topology::Index;
use crate::utils::ids::record_key;
use guru_worker_config::{
    Config, Forwarding, ForwardingTo, ListenAs, LoadBalanceGroup, LogConfig, RelayHost,
    RelayProtocol, Remote, TcpProxyProtocol,
};
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, thiserror::Error)]
pub enum DeriveError {
    #[error("entry {node} uses TLS, which needs certificates from a later stage")]
    TlsNotYetSupported { node: String },
    #[error(
        "relay {node} uses protocol {protocol}, which needs the internal CA from a later stage"
    )]
    RelayProtocolNotYetSupported {
        node: String,
        protocol: &'static str,
    },
    #[error("node {node} cannot terminate a destination path")]
    UnsupportedSpec { node: String },
    #[error("pod {node}: the import/export chain on one of its ports is not connected through")]
    DanglingBoundary { node: String },
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

/// A server's ideal config: what the canvas says it should serve, ignoring what
/// the rest of the fabric is currently running.
#[derive(Debug, Clone)]
pub struct DerivedConfig {
    pub config: Config,
    /// Index-aligned with `config.forwardings`.
    pub forwardings: Vec<ForwardingDeps>,
    /// Pods that could not be derived. The rest of `config` is unaffected.
    pub invalid: Vec<InvalidPod>,
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

    let index = Index::build(topology);
    let mut forwardings = Vec::new();
    let mut deps = Vec::new();
    let mut invalid = Vec::new();

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
        // Unattributable rather than pod-level: the ip record is what says which
        // server owns this pod, so a dangling link cannot be blamed on one server.
        // The schema rejects such a link, which is why this stays a hard failure.
        let ip = index
            .ip(&record_key(&cfg.ip.0))
            .ok_or_else(|| DeriveError::MissingIpRecord {
                node: pod.node.name.clone(),
                ip: record_key(&cfg.ip.0),
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
        if index.edge_on(listen_port).is_none() || index.edge_on(destination_port).is_none() {
            // A pod with an unconnected port is reported as a warning and skipped.
            continue;
        }

        match derive_pod(&index, pod, cfg, ip, listen_port, destination_port) {
            Ok((forwarding, pod_deps)) => {
                forwardings.push(forwarding);
                deps.push(pod_deps);
            }
            Err(error) => invalid.push(InvalidPod {
                node: pod.node.id.clone(),
                pod: pod.node.name.clone(),
                listen: format!("{}:{}", ip.ip, cfg.port),
                error: error.to_string(),
            }),
        }
    }

    let config = Config {
        ipv6_resolve: server_row.ipv6_resolve.into(),
        log: LogConfig {
            level: server_row.log_level.clone(),
        },
        forwardings,
    };
    // Every entry validated itself in `derive_pod`; what is left is the cross-pod
    // rule, which no single pod can be blamed for.
    config.validate()?;
    Ok(DerivedConfig {
        config,
        forwardings: deps,
        invalid,
    })
}

/// One pod's `[[forwarding]]` entry, or why that pod alone cannot be derived.
fn derive_pod(
    index: &Index<'_>,
    pod: &NodeWithPorts,
    cfg: &PodConfig,
    ip: &ServerIpRecordEntity,
    listen_port: &PortEntity,
    destination_port: &PortEntity,
) -> Result<(Forwarding, ForwardingDeps), DeriveError> {
    let address: IpAddr = ip.ip.parse().map_err(|_| DeriveError::InvalidIp {
        ip: record_key(&ip.id.0),
        value: ip.ip.clone(),
    })?;
    let (listen_as, listen_protocol, receive_proxy_protocol) =
        derive_listen(index, pod, listen_port)?;

    let producer = index
        .peer(destination_port)
        .ok_or_else(|| DeriveError::DanglingBoundary {
            node: pod.node.name.clone(),
        })?;
    let mut visited = Vec::new();
    let mut points_at = Vec::new();
    let to = derive_destination(index, producer, &mut visited, &mut points_at)?;

    let forwarding = Forwarding {
        tag: pod.node.name.clone(),
        listen: SocketAddr::new(address, cfg.port),
        receive_proxy_protocol,
        listen_as,
        to,
    };
    forwarding.validate()?;
    let deps = ForwardingDeps {
        pod: pod.node.id.clone(),
        serves: ListenerCap {
            ip: ip.ip.clone(),
            port: i64::from(cfg.port),
            protocol: listen_protocol,
        },
        points_at,
    };
    Ok((forwarding, deps))
}

/// The listen side of one pod: what the node feeding its `listen` port makes it.
fn derive_listen(
    index: &Index<'_>,
    pod: &NodeWithPorts,
    listen_port: &PortEntity,
) -> Result<(ListenAs, ListenProtocol, Option<TcpProxyProtocol>), DeriveError> {
    let consumer = index
        .peer(listen_port)
        .ok_or_else(|| DeriveError::DanglingBoundary {
            node: pod.node.name.clone(),
        })?;
    match &consumer.node.spec {
        NodeSpec::Entry(entry) => {
            if entry.tls.is_some() {
                return Err(DeriveError::TlsNotYetSupported {
                    node: consumer.node.name.clone(),
                });
            }
            Ok((
                ListenAs::Raw,
                ListenProtocol::Raw,
                entry.receive_proxy_protocol.map(Into::into),
            ))
        }
        NodeSpec::Relay(relay) => match relay.protocol {
            // The worker auto-detects PROXY on relay ingest.
            EntityRelayProtocol::TcpRaw => Ok((
                ListenAs::Relay(RelayHost::Tcp),
                ListenProtocol::RelayTcp,
                None,
            )),
            other => Err(DeriveError::RelayProtocolNotYetSupported {
                node: consumer.node.name.clone(),
                protocol: relay_protocol_name(other),
            }),
        },
        _ => Err(DeriveError::UnsupportedSpec {
            node: consumer.node.name.clone(),
        }),
    }
}

/// Walks the destination side of one pod, collecting into `points_at` every
/// listener on another server this forwarding will dial.
fn derive_destination(
    index: &Index<'_>,
    node: &NodeWithPorts,
    visited: &mut Vec<String>,
    points_at: &mut Vec<ListenerCap>,
) -> Result<ForwardingTo, DeriveError> {
    let key = record_key(&node.node.id.0);
    if visited.contains(&key) {
        return Err(DeriveError::Cycle {
            node: node.node.name.clone(),
        });
    }
    visited.push(key);

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
            let pod = listen_port.and_then(|p| index.peer(p));
            let Some(pod) = pod else {
                return Err(DeriveError::RelayWithoutPod {
                    node: node.node.name.clone(),
                });
            };
            let NodeSpec::Pod(pod_cfg) = &pod.node.spec else {
                return Err(DeriveError::RelayWithoutPod {
                    node: node.node.name.clone(),
                });
            };
            let pod_ip = index.ip(&record_key(&pod_cfg.ip.0)).ok_or_else(|| {
                DeriveError::MissingIpRecord {
                    node: pod.node.name.clone(),
                    ip: record_key(&pod_cfg.ip.0),
                }
            })?;
            // The override says *how* to reach the pod; the pod's own socket is
            // what identifies the listener we depend on.
            points_at.push(ListenerCap {
                ip: pod_ip.ip.clone(),
                port: i64::from(pod_cfg.port),
                protocol: ListenProtocol::RelayTcp,
            });
            let host = cfg
                .override_ip_address
                .clone()
                .unwrap_or_else(|| pod_ip.ip.clone());
            let port = cfg.override_port.unwrap_or(pod_cfg.port);
            // An IP literal becomes a socket address directly: an unbracketed IPv6
            // host would otherwise round-trip through `Remote::parse` as a domain
            // name and be handed to the worker's resolver. `override_ip_address` is
            // a free-form string, so a genuine hostname still takes the parse path.
            let destination = match host.parse::<IpAddr>() {
                Ok(address) => Remote::Address(SocketAddr::new(address, port)),
                Err(_) => Remote::parse(&format!("{host}:{port}")).map_err(|_| {
                    DeriveError::InvalidDestination {
                        node: node.node.name.clone(),
                        destination: format!("{host}:{port}"),
                    }
                })?,
            };
            ForwardingTo::Relay {
                protocol,
                destination,
                sni: None,
            }
        }
        NodeSpec::LoadBalanceDistribute(cfg) => {
            let mut members = smallvec::SmallVec::new();
            for port in inputs_in_order(node) {
                let Some(member) = index.peer(port) else {
                    continue; // unconnected members are skipped
                };
                members.push(derive_destination(index, member, visited, points_at)?);
            }
            ForwardingTo::LoadBalance(Box::new(LoadBalanceGroup {
                strategy: cfg.mode.into(),
                members,
            }))
        }
        NodeSpec::LoadBalanceAggregate(_) => {
            let port = inputs_in_order(node).into_iter().next().ok_or_else(|| {
                DeriveError::UnsupportedSpec {
                    node: node.node.name.clone(),
                }
            })?;
            let Some(source) = index.peer(port) else {
                return Err(DeriveError::UnsupportedSpec {
                    node: node.node.name.clone(),
                });
            };
            derive_destination(index, source, visited, points_at)?
        }
        // Boundary nodes are never reached: `Index::peer` resolves through them.
        NodeSpec::Pod(_)
        | NodeSpec::Entry(_)
        | NodeSpec::CanvasImport(_)
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
