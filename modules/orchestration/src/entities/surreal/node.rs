use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use crate::entities::surreal::server::ServerId;
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(NodeId, "orchestration_node");

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeEntity {
    pub id: NodeId,
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub server: Option<ServerId>,
    pub position: CanvasUiPosition,
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(tag = "type", content = "config", rename_all = "snake_case")]
pub enum NodeSpec {
    Pod(PodConfig),
    Entry(EntryConfig),
    Relay(RelayConfig),
    Exit(ExitConfig),
    LoadBalanceDistribute(LoadBalanceDistributeConfig),
    LoadBalanceAggregate(LoadBalanceAggregateConfig),
}

#[derive(Debug, Clone, SurrealValue)]
pub struct PodConfig {
    pub listen_address: String,
    pub listen_port: u16,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct EntryConfig {
    pub receive_proxy_protocol: Option<ProxyProtocolVersion>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct TlsConfig {
}

#[derive(Debug, Clone, SurrealValue, Copy, PartialEq, Eq)]
pub enum ProxyProtocolVersion {
    V1,
    V2,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct RelayConfig {
    pub proxy_protocol_negotiation: ProxyProtocolVersion,
    pub protocol: RelayProtocol,
    pub override_ip_address: Option<String>,
    pub override_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
pub enum RelayProtocol {
    TcpRaw,
    TcpTls,
    Quic,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct ExitConfig {
    pub destination: String,
    pub pass_proxy_protocol: Option<ProxyProtocolVersion>,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct LoadBalanceDistributeConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct LoadBalanceAggregateConfig {}
