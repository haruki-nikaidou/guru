use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use crate::entities::surreal::connection::EdgeConnectionId;
use crate::entities::surreal::dns::DnsProviderId;
use crate::entities::surreal::port::{PortDirection, PortEntity, PortId, PortKind};
use crate::entities::surreal::server::ServerIpRecordId;
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(NodeId, "orchestration_node");

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeEntity {
    pub id: NodeId,
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub position: CanvasUiPosition,
    pub created_rev: i64,
    pub retired_rev: Option<i64>,
    pub replaces: Option<NodeId>,
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(tag = "type", content = "config", rename_all = "snake_case")]
pub enum NodeSpec {
    CanvasExport(CanvasExportConfig),
    CanvasImport(CanvasImportConfig),
    Pod(PodConfig),
    Entry(EntryConfig),
    Relay(RelayConfig),
    Exit(ExitConfig),
    LoadBalanceDistribute(LoadBalanceDistributeConfig),
    LoadBalanceAggregate(LoadBalanceAggregateConfig),
}

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasExportConfig {
    pub kind: PortKind,
    pub direction: CanvasExportAs,
}

#[derive(Debug, Clone, SurrealValue, Copy, PartialEq, Eq)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum CanvasExportAs {
    InputIntoCanvas,
    OutputOutOfCanvas,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasImportConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct PodConfig {
    /// The ip record this pod listens on; it pins the pod to exactly one server.
    pub ip: ServerIpRecordId,
    pub port: u16,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct EntryConfig {
    pub receive_proxy_protocol: Option<ProxyProtocolVersion>,
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, SurrealValue)]
/// By setting this, the master node will acquire a TLS certificate and send it to worker nodes.
pub struct TlsConfig {
    /// The SNI of the TLS certificate
    pub sni: String,

    /// Which DNS provider to use for the TLS certificate
    pub dns_provider: DnsProviderId,

    /// The identifier of the domain
    /// - Cloudflare: zone ID
    /// - vercel: domain SLD
    pub domain_id: String,

    /// The URL of the ACME directory.
    ///
    /// eg. <https://acme-staging-v02.api.letsencrypt.org/directory>
    pub acme_directory: String,
}

#[derive(Debug, Clone, SurrealValue, Copy, PartialEq, Eq)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum ProxyProtocolVersion {
    V1,
    V2,
}

impl From<ProxyProtocolVersion> for guru_worker_config::TcpProxyProtocol {
    fn from(value: ProxyProtocolVersion) -> Self {
        match value {
            ProxyProtocolVersion::V1 => guru_worker_config::TcpProxyProtocol::V1,
            ProxyProtocolVersion::V2 => guru_worker_config::TcpProxyProtocol::V2,
        }
    }
}

#[derive(Debug, Clone, SurrealValue)]
pub struct RelayConfig {
    pub protocol: RelayProtocol,
    pub override_ip_address: Option<String>,
    pub override_port: Option<u16>,
}

#[derive(Debug, Clone, Copy, SurrealValue, PartialEq, Eq)]
#[surreal(untagged, rename_all = "snake_case")]
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
pub struct LoadBalanceDistributeConfig {
    pub mode: LoadBalanceMode,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct LoadBalanceAggregateConfig {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum LoadBalanceMode {
    RoundRobin,
    Random,
    IpHash,
    Fallback,
}

impl From<LoadBalanceMode> for guru_worker_config::LoadBalanceStrategy {
    fn from(value: LoadBalanceMode) -> Self {
        match value {
            LoadBalanceMode::RoundRobin => guru_worker_config::LoadBalanceStrategy::RoundRobin,
            LoadBalanceMode::Random => guru_worker_config::LoadBalanceStrategy::Random,
            LoadBalanceMode::IpHash => guru_worker_config::LoadBalanceStrategy::IpHash,
            LoadBalanceMode::Fallback => guru_worker_config::LoadBalanceStrategy::Fallback,
        }
    }
}

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeWithPorts {
    pub node: NodeEntity,
    pub ports: Vec<PortEntity>,
}

/// A port to create together with its node.
#[derive(Debug, Clone, SurrealValue)]
pub struct NewPort {
    pub kind: PortKind,
    pub direction: PortDirection,
    pub key: String,
    pub position: i64,
}

/// An edge that survives a node replacement, re-attached to the port carrying the
/// same `key` on the replacement node.
#[derive(Debug, Clone, SurrealValue)]
pub struct CarryEdge {
    pub old_edge: EdgeConnectionId,
    pub new_port_key: String,
    pub other_port: PortId,
    pub new_port_is_source: bool,
}

pub struct CreateNodeRow {
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub position: CanvasUiPosition,
    pub created_rev: i64,
    pub replaces: Option<NodeId>,
    pub ports: Vec<NewPort>,
}

impl Processor<CreateNodeRow> for SurrealProcessor {
    type Output = NodeWithPorts;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:CreateNodeRow", skip_all, err)]
    async fn process(&self, input: CreateNodeRow) -> Result<Self::Output, Self::Error> {
        // Statement 0 is BEGIN; the RETURN below is statement 3.
        let mut resp = self
            .db()
            .query(
                "BEGIN TRANSACTION;
                 LET $node = (CREATE ONLY orchestration_node CONTENT {
                     canvas: $canvas, name: $name, comment: $comment, spec: $spec,
                     position: $position, created_rev: $created_rev, retired_rev: NONE,
                     replaces: $replaces
                 });
                 LET $ports = (INSERT INTO orchestration_port
                     (SELECT $node.id AS owner, kind, direction, key, position FROM $new_ports)
                     RETURN AFTER);
                 RETURN { node: $node, ports: $ports };
                 COMMIT TRANSACTION;",
            )
            .bind(("canvas", input.canvas))
            .bind(("name", input.name))
            .bind(("comment", input.comment))
            .bind(("spec", input.spec))
            .bind(("position", input.position))
            .bind(("created_rev", input.created_rev))
            .bind(("replaces", input.replaces))
            .bind(("new_ports", input.ports))
            .await?;
        resp.take::<Option<NodeWithPorts>>(3)?
            .ok_or_else(|| surrealdb::Error::internal("create node returned no row".to_string()))
    }
}

pub struct FindNodeById {
    pub id: NodeId,
}

impl Processor<FindNodeById> for SurrealProcessor {
    type Output = Option<NodeEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindNodeById", skip_all, err)]
    async fn process(&self, input: FindNodeById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<NodeEntity>>(0)
    }
}

pub struct FindNodeWithPorts {
    pub id: NodeId,
}

impl Processor<FindNodeWithPorts> for SurrealProcessor {
    type Output = Option<NodeWithPorts>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindNodeWithPorts", skip_all, err)]
    async fn process(&self, input: FindNodeWithPorts) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id.clone()))
            .query("SELECT * FROM orchestration_port WHERE owner = $id ORDER BY position")
            .bind(("id", input.id))
            .await?;
        let Some(node) = resp.take::<Option<NodeEntity>>(0)? else {
            return Ok(None);
        };
        let ports = resp.take::<Vec<PortEntity>>(1)?;
        Ok(Some(NodeWithPorts { node, ports }))
    }
}

pub struct UpdateNodeMetaRow {
    pub id: NodeId,
    pub name: String,
    pub comment: String,
    pub position: CanvasUiPosition,
}

impl Processor<UpdateNodeMetaRow> for SurrealProcessor {
    type Output = NodeEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateNodeMetaRow", skip_all, err)]
    async fn process(&self, input: UpdateNodeMetaRow) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("UPDATE $id SET name = $name, comment = $comment, position = $position RETURN AFTER")
            .bind(("id", input.id))
            .bind(("name", input.name))
            .bind(("comment", input.comment))
            .bind(("position", input.position))
            .await?;
        resp.take::<Option<NodeEntity>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("node not found".to_string()))
    }
}

pub struct RetireNodeRow {
    pub id: NodeId,
    pub revision: i64,
}

impl Processor<RetireNodeRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:RetireNodeRow", skip_all, err)]
    async fn process(&self, input: RetireNodeRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "BEGIN TRANSACTION;
                 LET $ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $id);
                 UPDATE orchestration_node SET retired_rev = $revision
                     WHERE id = $id AND retired_rev IS NONE;
                 UPDATE orchestration_edge_connection SET retired_rev = $revision
                     WHERE retired_rev IS NONE AND (in IN $ports OR out IN $ports);
                 COMMIT TRANSACTION;",
            )
            .bind(("id", input.id))
            .bind(("revision", input.revision))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct ForceDeleteNodeRow {
    pub id: NodeId,
}

impl Processor<ForceDeleteNodeRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:ForceDeleteNodeRow", skip_all, err)]
    async fn process(&self, input: ForceDeleteNodeRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "BEGIN TRANSACTION;
                 LET $ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $id);
                 DELETE orchestration_edge_connection WHERE in IN $ports OR out IN $ports;
                 DELETE orchestration_port WHERE id IN $ports;
                 DELETE $id;
                 COMMIT TRANSACTION;",
            )
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct ReplaceNodeRow {
    pub old: NodeId,
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub position: CanvasUiPosition,
    pub revision: i64,
    pub ports: Vec<NewPort>,
    pub carry: Vec<CarryEdge>,
}

impl Processor<ReplaceNodeRow> for SurrealProcessor {
    type Output = NodeWithPorts;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:ReplaceNodeRow", skip_all, err)]
    async fn process(&self, input: ReplaceNodeRow) -> Result<Self::Output, Self::Error> {
        // Statement 0 is BEGIN; the RETURN below is statement 7.
        let mut resp = self
            .db()
            .query(
                "BEGIN TRANSACTION;
                 LET $node = (CREATE ONLY orchestration_node CONTENT {
                     canvas: $canvas, name: $name, comment: $comment, spec: $spec,
                     position: $position, created_rev: $revision, retired_rev: NONE,
                     replaces: $old
                 });
                 LET $ports = (INSERT INTO orchestration_port
                     (SELECT $node.id AS owner, kind, direction, key, position FROM $new_ports)
                     RETURN AFTER);
                 LET $old_ports = (SELECT VALUE id FROM orchestration_port WHERE owner = $old);
                 UPDATE orchestration_node SET retired_rev = $revision
                     WHERE id = $old AND retired_rev IS NONE;
                 UPDATE orchestration_edge_connection SET retired_rev = $revision
                     WHERE retired_rev IS NONE AND (in IN $old_ports OR out IN $old_ports);
                 FOR $c IN $carry {
                     LET $np = (SELECT VALUE id FROM $ports WHERE key = $c.new_port_key)[0];
                     LET $other = $c.other_port;
                     IF $c.new_port_is_source {
                         RELATE $np->orchestration_edge_connection->$other
                             CONTENT { created_rev: $revision, retired_rev: NONE };
                     } ELSE {
                         RELATE $other->orchestration_edge_connection->$np
                             CONTENT { created_rev: $revision, retired_rev: NONE };
                     };
                 };
                 RETURN { node: $node, ports: $ports };
                 COMMIT TRANSACTION;",
            )
            .bind(("old", input.old))
            .bind(("canvas", input.canvas))
            .bind(("name", input.name))
            .bind(("comment", input.comment))
            .bind(("spec", input.spec))
            .bind(("position", input.position))
            .bind(("revision", input.revision))
            .bind(("new_ports", input.ports))
            .bind(("carry", input.carry))
            .await?;
        resp.take::<Option<NodeWithPorts>>(7)?
            .ok_or_else(|| surrealdb::Error::internal("replace node returned no row".to_string()))
    }
}
