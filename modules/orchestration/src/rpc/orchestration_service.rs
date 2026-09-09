//! The operator-facing `Orchestration` gRPC service.
//!
//! Handlers are thin: decode ids and specs, call a service, encode the reply. All
//! rules live in `services`.

use crate::entities::surreal::canvas::{CanvasEntity, CanvasUiPosition};
use crate::entities::surreal::connection::EdgeConnectionEntity;
use crate::entities::surreal::node::{
    CanvasExportAs, CanvasExportConfig, CanvasImportConfig, EntryConfig, ExitConfig,
    LoadBalanceAggregateConfig, LoadBalanceDistributeConfig, LoadBalanceMode, NodeEntity, NodeSpec,
    NodeWithPorts, PodConfig, ProxyProtocolVersion, RelayConfig, RelayProtocol, TlsConfig,
};
use crate::entities::surreal::port::{PortDirection, PortEntity, PortKind};
use crate::entities::surreal::server::{ServerIpRecordEntity, ServerIpv6Resolve, ServerWithIp};
use crate::entities::surreal::view::{ConfigSnapshot, ListenProtocol, ListenerCap};
use crate::services::canvas::{self, CanvasService};
use crate::services::edge::{self, EdgeService};
use crate::services::node::{self, NodeService};
use crate::services::rollout::{self, RolloutService};
use crate::services::server::{self, ServerService};
use crate::services::topology::{ProblemKind, ProblemSeverity, TopologyProblem};
use crate::utils::ids;
use kanau::processor::Processor;
use rpguru_sdk::orchestration as pb;
use tonic::{Request, Response, Status};

#[derive(Clone)]
pub struct OrchestrationGrpc {
    pub canvases: CanvasService,
    pub servers: ServerService,
    pub nodes: NodeService,
    pub edges: EdgeService,
    pub rollout: RolloutService,
}

// --- encoding ---------------------------------------------------------------

fn position_to_proto(position: CanvasUiPosition) -> pb::CanvasUiPosition {
    pb::CanvasUiPosition {
        x: position.x,
        y: position.y,
    }
}

fn position_from_proto(position: Option<pb::CanvasUiPosition>) -> CanvasUiPosition {
    let position = position.unwrap_or_default();
    CanvasUiPosition {
        x: position.x,
        y: position.y,
    }
}

fn canvas_to_proto(canvas: &CanvasEntity) -> pb::Canvas {
    pb::Canvas {
        id: ids::record_key(&canvas.id.0),
        name: canvas.name.clone(),
        description: canvas.description.clone(),
    }
}

fn ip_to_proto(ip: &ServerIpRecordEntity) -> pb::ServerIp {
    pb::ServerIp {
        id: ids::record_key(&ip.id.0),
        server_id: ids::record_key(&ip.server.0),
        ip: ip.ip.clone(),
        country: ip.country.clone(),
    }
}

fn ipv6_to_proto(value: ServerIpv6Resolve) -> i32 {
    match value {
        ServerIpv6Resolve::Required => pb::Ipv6Resolve::Ipv6Required,
        ServerIpv6Resolve::Preferred => pb::Ipv6Resolve::Ipv6Preferred,
        ServerIpv6Resolve::Tolerated => pb::Ipv6Resolve::Ipv6Tolerated,
        ServerIpv6Resolve::Forbidden => pb::Ipv6Resolve::Ipv6Forbidden,
    }
    .into()
}

fn ipv6_from_proto(value: i32) -> ServerIpv6Resolve {
    match pb::Ipv6Resolve::try_from(value) {
        Ok(pb::Ipv6Resolve::Ipv6Required) => ServerIpv6Resolve::Required,
        Ok(pb::Ipv6Resolve::Ipv6Preferred) => ServerIpv6Resolve::Preferred,
        Ok(pb::Ipv6Resolve::Ipv6Forbidden) => ServerIpv6Resolve::Forbidden,
        // Unspecified means "the default policy".
        _ => ServerIpv6Resolve::Tolerated,
    }
}

fn server_to_proto(server: &ServerWithIp) -> pb::Server {
    pb::Server {
        id: ids::record_key(&server.server.id.0),
        canvas_id: ids::record_key(&server.server.canvas.0),
        name: server.server.name.clone(),
        icon: server.server.icon.clone(),
        comment: server.server.comment.clone(),
        position: Some(position_to_proto(server.server.position)),
        ipv6_resolve: ipv6_to_proto(server.server.ipv6_resolve),
        log_level: server.server.log_level.clone(),
        last_seen_at: server
            .server
            .last_seen_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_default(),
        ips: server.ips.iter().map(ip_to_proto).collect(),
    }
}

fn port_to_proto(port: &PortEntity) -> pb::Port {
    pb::Port {
        id: ids::record_key(&port.id.0),
        node_id: ids::record_key(&port.owner.0),
        kind: match port.kind {
            PortKind::DeriveListen => pb::PortKind::DeriveListen,
            PortKind::DeriveDestination => pb::PortKind::DeriveDestination,
        }
        .into(),
        direction: match port.direction {
            PortDirection::Input => pb::PortDirection::PortInput,
            PortDirection::Output => pb::PortDirection::PortOutput,
        }
        .into(),
        key: port.key.clone(),
        position: port.position,
    }
}

fn proxy_to_proto(value: Option<ProxyProtocolVersion>) -> i32 {
    match value {
        None => pb::ProxyProtocolVersion::Unspecified,
        Some(ProxyProtocolVersion::V1) => pb::ProxyProtocolVersion::ProxyV1,
        Some(ProxyProtocolVersion::V2) => pb::ProxyProtocolVersion::ProxyV2,
    }
    .into()
}

fn proxy_from_proto(value: i32) -> Option<ProxyProtocolVersion> {
    match pb::ProxyProtocolVersion::try_from(value) {
        Ok(pb::ProxyProtocolVersion::ProxyV1) => Some(ProxyProtocolVersion::V1),
        Ok(pb::ProxyProtocolVersion::ProxyV2) => Some(ProxyProtocolVersion::V2),
        _ => None,
    }
}

fn spec_to_proto(spec: &NodeSpec) -> pb::NodeSpec {
    use pb::node_spec::Spec;
    let spec = match spec {
        NodeSpec::Pod(cfg) => Spec::Pod(pb::PodConfig {
            ip_record_id: ids::record_key(&cfg.ip.0),
            port: u32::from(cfg.port),
        }),
        NodeSpec::Entry(cfg) => Spec::Entry(pb::EntryConfig {
            receive_proxy_protocol: proxy_to_proto(cfg.receive_proxy_protocol),
            tls: cfg.tls.as_ref().map(|tls| pb::TlsConfig {
                sni: tls.sni.clone(),
                dns_provider_id: ids::record_key(&tls.dns_provider.0),
                domain_id: tls.domain_id.clone(),
                acme_directory: tls.acme_directory.clone(),
            }),
        }),
        NodeSpec::Relay(cfg) => Spec::Relay(pb::RelayConfig {
            protocol: match cfg.protocol {
                RelayProtocol::TcpRaw => pb::RelayProtocol::RelayTcpRaw,
                RelayProtocol::TcpTls => pb::RelayProtocol::RelayTcpTls,
                RelayProtocol::Quic => pb::RelayProtocol::RelayQuic,
            }
            .into(),
            override_ip_address: cfg.override_ip_address.clone().unwrap_or_default(),
            override_port: cfg.override_port.map(u32::from).unwrap_or_default(),
        }),
        NodeSpec::Exit(cfg) => Spec::Exit(pb::ExitConfig {
            destination: cfg.destination.clone(),
            pass_proxy_protocol: proxy_to_proto(cfg.pass_proxy_protocol),
        }),
        NodeSpec::LoadBalanceDistribute(cfg) => {
            Spec::LoadBalanceDistribute(pb::LoadBalanceDistributeConfig {
                mode: match cfg.mode {
                    LoadBalanceMode::RoundRobin => pb::LoadBalanceMode::RoundRobin,
                    LoadBalanceMode::Random => pb::LoadBalanceMode::Random,
                    LoadBalanceMode::IpHash => pb::LoadBalanceMode::IpHash,
                    LoadBalanceMode::Fallback => pb::LoadBalanceMode::Fallback,
                }
                .into(),
            })
        }
        NodeSpec::LoadBalanceAggregate(_) => {
            Spec::LoadBalanceAggregate(pb::LoadBalanceAggregateConfig {})
        }
        NodeSpec::CanvasImport(_) => Spec::CanvasImport(pb::CanvasImportConfig {
            canvas_id: String::new(),
        }),
        NodeSpec::CanvasExport(cfg) => Spec::CanvasExport(pb::CanvasExportConfig {
            kind: match cfg.kind {
                PortKind::DeriveListen => pb::PortKind::DeriveListen,
                PortKind::DeriveDestination => pb::PortKind::DeriveDestination,
            }
            .into(),
            direction: match cfg.direction {
                CanvasExportAs::InputIntoCanvas => pb::CanvasExportAs::InputIntoCanvas,
                CanvasExportAs::OutputOutOfCanvas => pb::CanvasExportAs::OutputOutOfCanvas,
            }
            .into(),
        }),
    };
    pb::NodeSpec { spec: Some(spec) }
}

fn spec_from_proto(spec: Option<pb::NodeSpec>) -> Result<NodeSpec, Status> {
    use pb::node_spec::Spec;
    let spec = spec
        .and_then(|s| s.spec)
        .ok_or_else(|| Status::invalid_argument("spec is required"))?;
    Ok(match spec {
        Spec::Pod(cfg) => NodeSpec::Pod(PodConfig {
            ip: ids::server_ip_id(&cfg.ip_record_id),
            port: u16::try_from(cfg.port)
                .map_err(|_| Status::invalid_argument("port out of range"))?,
        }),
        Spec::Entry(cfg) => NodeSpec::Entry(EntryConfig {
            receive_proxy_protocol: proxy_from_proto(cfg.receive_proxy_protocol),
            tls: cfg.tls.map(|tls| TlsConfig {
                sni: tls.sni,
                dns_provider: ids::dns_provider_id(&tls.dns_provider_id),
                domain_id: tls.domain_id,
                acme_directory: tls.acme_directory,
            }),
        }),
        Spec::Relay(cfg) => NodeSpec::Relay(RelayConfig {
            protocol: match pb::RelayProtocol::try_from(cfg.protocol) {
                Ok(pb::RelayProtocol::RelayTcpTls) => RelayProtocol::TcpTls,
                Ok(pb::RelayProtocol::RelayQuic) => RelayProtocol::Quic,
                _ => RelayProtocol::TcpRaw,
            },
            override_ip_address: (!cfg.override_ip_address.is_empty())
                .then_some(cfg.override_ip_address),
            override_port: match cfg.override_port {
                0 => None,
                port => Some(
                    u16::try_from(port)
                        .map_err(|_| Status::invalid_argument("override_port out of range"))?,
                ),
            },
        }),
        Spec::Exit(cfg) => NodeSpec::Exit(ExitConfig {
            destination: cfg.destination,
            pass_proxy_protocol: proxy_from_proto(cfg.pass_proxy_protocol),
        }),
        Spec::LoadBalanceDistribute(cfg) => {
            NodeSpec::LoadBalanceDistribute(LoadBalanceDistributeConfig {
                mode: match pb::LoadBalanceMode::try_from(cfg.mode) {
                    Ok(pb::LoadBalanceMode::Random) => LoadBalanceMode::Random,
                    Ok(pb::LoadBalanceMode::IpHash) => LoadBalanceMode::IpHash,
                    Ok(pb::LoadBalanceMode::Fallback) => LoadBalanceMode::Fallback,
                    _ => LoadBalanceMode::RoundRobin,
                },
            })
        }
        Spec::LoadBalanceAggregate(_) => {
            NodeSpec::LoadBalanceAggregate(LoadBalanceAggregateConfig {})
        }
        Spec::CanvasImport(_) => NodeSpec::CanvasImport(CanvasImportConfig {}),
        Spec::CanvasExport(cfg) => NodeSpec::CanvasExport(CanvasExportConfig {
            kind: match pb::PortKind::try_from(cfg.kind) {
                Ok(pb::PortKind::DeriveListen) => PortKind::DeriveListen,
                _ => PortKind::DeriveDestination,
            },
            direction: match pb::CanvasExportAs::try_from(cfg.direction) {
                Ok(pb::CanvasExportAs::OutputOutOfCanvas) => CanvasExportAs::OutputOutOfCanvas,
                _ => CanvasExportAs::InputIntoCanvas,
            },
        }),
    })
}

fn node_row_to_proto(node: &NodeEntity, ports: &[PortEntity]) -> pb::Node {
    pb::Node {
        id: ids::record_key(&node.id.0),
        canvas_id: ids::record_key(&node.canvas.0),
        name: node.name.clone(),
        comment: node.comment.clone(),
        spec: Some(spec_to_proto(&node.spec)),
        position: Some(position_to_proto(node.position)),
        ports: ports.iter().map(port_to_proto).collect(),
    }
}

fn node_to_proto(node: &NodeWithPorts) -> pb::Node {
    node_row_to_proto(&node.node, &node.ports)
}

fn edge_to_proto(edge: &EdgeConnectionEntity) -> pb::Edge {
    pb::Edge {
        id: ids::record_key(&edge.id.0),
        source_port_id: ids::record_key(&edge.source.0),
        target_port_id: ids::record_key(&edge.target.0),
    }
}

fn listener_cap_to_proto(cap: &ListenerCap) -> pb::ListenerCap {
    pb::ListenerCap {
        ip: cap.ip.clone(),
        port: u32::try_from(cap.port).unwrap_or_default(),
        protocol: match cap.protocol {
            ListenProtocol::Raw => "raw",
            ListenProtocol::RelayTcp => "relay_tcp",
            ListenProtocol::RelayTls => "relay_tls",
            ListenProtocol::RelayQuic => "relay_quic",
        }
        .to_string(),
    }
}

fn snapshot_to_proto(snapshot: &ConfigSnapshot) -> pb::ConfigSnapshot {
    pb::ConfigSnapshot {
        revision: snapshot.revision,
        created_at: snapshot.created_at.to_rfc3339(),
        forwardings: snapshot
            .forwardings
            .iter()
            .map(|deps| pb::ForwardingDeps {
                serves: Some(listener_cap_to_proto(&deps.serves)),
                points_at: deps.points_at.iter().map(listener_cap_to_proto).collect(),
            })
            .collect(),
    }
}

fn problem_to_proto(problem: &TopologyProblem) -> pb::Problem {
    pb::Problem {
        severity: match problem.severity {
            ProblemSeverity::Error => pb::ProblemSeverity::ProblemError,
            ProblemSeverity::Warning => pb::ProblemSeverity::ProblemWarning,
        }
        .into(),
        kind: match problem.kind {
            ProblemKind::PortKindMismatch => pb::ProblemKind::PortKindMismatch,
            ProblemKind::EdgeDirectionInvalid => pb::ProblemKind::EdgeDirectionInvalid,
            ProblemKind::EdgeSelfNode => pb::ProblemKind::EdgeSelfNode,
            ProblemKind::EdgeCrossCanvas => pb::ProblemKind::EdgeCrossCanvas,
            ProblemKind::PortOversubscribed => pb::ProblemKind::PortOversubscribed,
            ProblemKind::PortShapeInvalid => pb::ProblemKind::PortShapeInvalid,
            ProblemKind::Cycle => pb::ProblemKind::Cycle,
            ProblemKind::DuplicateListen => pb::ProblemKind::DuplicateListen,
            ProblemKind::PodIpForeign => pb::ProblemKind::PodIpForeign,
            ProblemKind::ExitDestinationInvalid => pb::ProblemKind::ExitDestinationInvalid,
            ProblemKind::IpHashWithoutClientIp => pb::ProblemKind::IpHashWithoutClientIp,
            ProblemKind::UnsupportedSpec => pb::ProblemKind::UnsupportedSpec,
            ProblemKind::PodPortUnconnected => pb::ProblemKind::PodPortUnconnected,
            ProblemKind::RelaySameServer => pb::ProblemKind::RelaySameServer,
            ProblemKind::DistributeSingleMember => pb::ProblemKind::DistributeSingleMember,
        }
        .into(),
        message: problem.message.clone(),
        node_ids: problem
            .nodes
            .iter()
            .map(|n| ids::record_key(&n.0))
            .collect(),
        edge_ids: problem
            .edges
            .iter()
            .map(|e| ids::record_key(&e.0))
            .collect(),
        port_ids: problem
            .ports
            .iter()
            .map(|p| ids::record_key(&p.0))
            .collect(),
    }
}

// --- handlers ---------------------------------------------------------------

#[tonic::async_trait]
impl pb::orchestration_server::Orchestration for OrchestrationGrpc {
    async fn create_canvas(
        &self,
        request: Request<pb::CreateCanvasRequest>,
    ) -> Result<Response<pb::CreateCanvasReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let canvas = self
            .canvases
            .process(canvas::CreateCanvas {
                actor,
                name: input.name,
                description: input.description,
            })
            .await?;
        Ok(Response::new(pb::CreateCanvasReply {
            canvas: Some(canvas_to_proto(&canvas)),
        }))
    }

    async fn list_canvases(
        &self,
        request: Request<pb::ListCanvasesRequest>,
    ) -> Result<Response<pb::ListCanvasesReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let canvases = self
            .canvases
            .process(canvas::ListCanvases { actor })
            .await?;
        Ok(Response::new(pb::ListCanvasesReply {
            canvases: canvases.iter().map(canvas_to_proto).collect(),
        }))
    }

    async fn get_canvas(
        &self,
        request: Request<pb::GetCanvasRequest>,
    ) -> Result<Response<pb::GetCanvasReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let contents = self
            .canvases
            .process(canvas::GetCanvas {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
            })
            .await?;
        Ok(Response::new(pb::GetCanvasReply {
            canvas: Some(canvas_to_proto(&contents.canvas)),
            servers: contents.servers.iter().map(server_to_proto).collect(),
            nodes: contents.nodes.iter().map(node_to_proto).collect(),
            edges: contents.edges.iter().map(edge_to_proto).collect(),
        }))
    }

    async fn update_canvas(
        &self,
        request: Request<pb::UpdateCanvasRequest>,
    ) -> Result<Response<pb::UpdateCanvasReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let canvas = self
            .canvases
            .process(canvas::UpdateCanvas {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
                name: input.name,
                description: input.description,
            })
            .await?;
        Ok(Response::new(pb::UpdateCanvasReply {
            canvas: Some(canvas_to_proto(&canvas)),
        }))
    }

    async fn delete_canvas(
        &self,
        request: Request<pb::DeleteCanvasRequest>,
    ) -> Result<Response<pb::DeleteCanvasReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.canvases
            .process(canvas::DeleteCanvas {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
            })
            .await?;
        Ok(Response::new(pb::DeleteCanvasReply {}))
    }

    async fn validate_canvas(
        &self,
        request: Request<pb::ValidateCanvasRequest>,
    ) -> Result<Response<pb::ValidateCanvasReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let problems = self
            .canvases
            .process(canvas::ValidateCanvas {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
            })
            .await?;
        Ok(Response::new(pb::ValidateCanvasReply {
            problems: problems.iter().map(problem_to_proto).collect(),
        }))
    }

    async fn create_server(
        &self,
        request: Request<pb::CreateServerRequest>,
    ) -> Result<Response<pb::CreateServerReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let server = self
            .servers
            .process(server::CreateServer {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
                name: input.name,
                icon: input.icon,
                comment: input.comment,
                position: position_from_proto(input.position),
                ipv6_resolve: ipv6_from_proto(input.ipv6_resolve),
                log_level: input.log_level,
            })
            .await?;
        Ok(Response::new(pb::CreateServerReply {
            server: Some(server_to_proto(&ServerWithIp {
                server,
                ips: Vec::new(),
            })),
        }))
    }

    async fn update_server(
        &self,
        request: Request<pb::UpdateServerRequest>,
    ) -> Result<Response<pb::UpdateServerReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let server = self
            .servers
            .process(server::UpdateServer {
                actor,
                server: ids::server_id(&input.server_id),
                name: input.name,
                icon: input.icon,
                comment: input.comment,
                ipv6_resolve: ipv6_from_proto(input.ipv6_resolve),
                log_level: input.log_level,
            })
            .await?;
        Ok(Response::new(pb::UpdateServerReply {
            server: Some(server_to_proto(&ServerWithIp {
                server,
                ips: Vec::new(),
            })),
        }))
    }

    async fn delete_server(
        &self,
        request: Request<pb::DeleteServerRequest>,
    ) -> Result<Response<pb::DeleteServerReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.servers
            .process(server::DeleteServer {
                actor,
                server: ids::server_id(&input.server_id),
            })
            .await?;
        Ok(Response::new(pb::DeleteServerReply {}))
    }

    async fn move_server(
        &self,
        request: Request<pb::MoveServerRequest>,
    ) -> Result<Response<pb::MoveServerReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.servers
            .process(server::MoveServer {
                actor,
                server: ids::server_id(&input.server_id),
                position: position_from_proto(input.position),
            })
            .await?;
        Ok(Response::new(pb::MoveServerReply {}))
    }

    async fn add_server_ip(
        &self,
        request: Request<pb::AddServerIpRequest>,
    ) -> Result<Response<pb::AddServerIpReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let ip = self
            .servers
            .process(server::AddServerIp {
                actor,
                server: ids::server_id(&input.server_id),
                ip: input.ip,
                country: input.country,
            })
            .await?;
        Ok(Response::new(pb::AddServerIpReply {
            ip: Some(ip_to_proto(&ip)),
        }))
    }

    async fn remove_server_ip(
        &self,
        request: Request<pb::RemoveServerIpRequest>,
    ) -> Result<Response<pb::RemoveServerIpReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.servers
            .process(server::RemoveServerIp {
                actor,
                ip_record: ids::server_ip_id(&input.ip_record_id),
            })
            .await?;
        Ok(Response::new(pb::RemoveServerIpReply {}))
    }

    async fn create_node(
        &self,
        request: Request<pb::CreateNodeRequest>,
    ) -> Result<Response<pb::CreateNodeReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let node = self
            .nodes
            .process(node::CreateNode {
                actor,
                canvas: ids::canvas_id(&input.canvas_id),
                name: input.name,
                comment: input.comment,
                spec: spec_from_proto(input.spec)?,
                position: position_from_proto(input.position),
                item_count: input.item_count,
            })
            .await?;
        Ok(Response::new(pb::CreateNodeReply {
            node: Some(node_to_proto(&node)),
        }))
    }

    async fn replace_node_spec(
        &self,
        request: Request<pb::ReplaceNodeSpecRequest>,
    ) -> Result<Response<pb::ReplaceNodeSpecReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let node = self
            .nodes
            .process(node::ReplaceNodeSpec {
                actor,
                node: ids::node_id(&input.node_id),
                spec: spec_from_proto(input.spec)?,
                item_count: input.item_count,
            })
            .await?;
        Ok(Response::new(pb::ReplaceNodeSpecReply {
            node: Some(node_to_proto(&node)),
        }))
    }

    async fn update_node_meta(
        &self,
        request: Request<pb::UpdateNodeMetaRequest>,
    ) -> Result<Response<pb::UpdateNodeMetaReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let node = self
            .nodes
            .process(node::UpdateNodeMeta {
                actor,
                node: ids::node_id(&input.node_id),
                name: input.name,
                comment: input.comment,
                position: position_from_proto(input.position),
            })
            .await?;
        Ok(Response::new(pb::UpdateNodeMetaReply {
            node: Some(node_row_to_proto(&node, &[])),
        }))
    }

    async fn retire_node(
        &self,
        request: Request<pb::RetireNodeRequest>,
    ) -> Result<Response<pb::RetireNodeReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.nodes
            .process(node::RetireNode {
                actor,
                node: ids::node_id(&input.node_id),
            })
            .await?;
        Ok(Response::new(pb::RetireNodeReply {}))
    }

    async fn force_delete_node(
        &self,
        request: Request<pb::ForceDeleteNodeRequest>,
    ) -> Result<Response<pb::ForceDeleteNodeReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.nodes
            .process(node::ForceDeleteNode {
                actor,
                node: ids::node_id(&input.node_id),
            })
            .await?;
        Ok(Response::new(pb::ForceDeleteNodeReply {}))
    }

    async fn connect_ports(
        &self,
        request: Request<pb::ConnectRequest>,
    ) -> Result<Response<pb::ConnectReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let edge = self
            .edges
            .process(edge::Connect {
                actor,
                output_port: ids::port_id(&input.output_port_id),
                input_port: ids::port_id(&input.input_port_id),
            })
            .await?;
        Ok(Response::new(pb::ConnectReply {
            edge: Some(edge_to_proto(&edge)),
        }))
    }

    async fn disconnect(
        &self,
        request: Request<pb::DisconnectRequest>,
    ) -> Result<Response<pb::DisconnectReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.edges
            .process(edge::Disconnect {
                actor,
                edge: ids::edge_id(&input.edge_id),
            })
            .await?;
        Ok(Response::new(pb::DisconnectReply {}))
    }

    async fn force_disconnect(
        &self,
        request: Request<pb::ForceDisconnectRequest>,
    ) -> Result<Response<pb::ForceDisconnectReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.edges
            .process(edge::ForceDisconnect {
                actor,
                edge: ids::edge_id(&input.edge_id),
            })
            .await?;
        Ok(Response::new(pb::ForceDisconnectReply {}))
    }

    async fn get_server_config(
        &self,
        request: Request<pb::GetServerConfigRequest>,
    ) -> Result<Response<pb::GetServerConfigReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let (revision, toml) = self
            .rollout
            .process(rollout::GetServerConfig {
                actor,
                server: ids::server_id(&input.server_id),
            })
            .await?;
        Ok(Response::new(pb::GetServerConfigReply { revision, toml }))
    }

    async fn get_server_rollout_status(
        &self,
        request: Request<pb::GetServerRolloutStatusRequest>,
    ) -> Result<Response<pb::GetServerRolloutStatusReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let status = self
            .rollout
            .process(rollout::GetServerRolloutStatus {
                actor,
                server: ids::server_id(&input.server_id),
            })
            .await?;
        Ok(Response::new(pb::GetServerRolloutStatusReply {
            desired: status.desired.as_ref().map(snapshot_to_proto),
            in_flight: status.in_flight.as_ref().map(snapshot_to_proto),
            applied: status.applied.as_ref().map(snapshot_to_proto),
            apply_error: status.apply_error.unwrap_or_default(),
            derive_error: status.derive_error.unwrap_or_default(),
            waiting_for_server_ids: status
                .waiting_for
                .iter()
                .map(|id| ids::record_key(&id.0))
                .collect(),
            derivation_pending: status.derivation_pending,
            last_seen_at: status
                .last_seen_at
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
        }))
    }

    async fn forget_server_applied(
        &self,
        request: Request<pb::ForgetServerAppliedRequest>,
    ) -> Result<Response<pb::ForgetServerAppliedReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        self.rollout
            .process(rollout::ForgetServerApplied {
                actor,
                server: ids::server_id(&input.server_id),
            })
            .await?;
        Ok(Response::new(pb::ForgetServerAppliedReply {}))
    }
}
