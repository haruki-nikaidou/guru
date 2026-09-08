#![allow(dead_code)]

use kanau::processor::Processor;
use orchestration::entities::surreal::canvas::{CanvasEntity, CanvasUiPosition, CreateCanvas};
use orchestration::entities::surreal::node::{
    CreateNodeRow, NewPort, NodeSpec, NodeWithPorts, PodConfig,
};
use orchestration::entities::surreal::port::{PortDirection, PortKind};
use orchestration::entities::surreal::server::{
    CreateServer, CreateServerIp, ServerEntity, ServerIpRecordEntity, ServerIpv6Resolve,
};
use wakuwaku::surreal::SurrealProcessor;

pub type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A fresh in-memory database with the module's real schema applied.
pub async fn setup() -> Result<SurrealProcessor, Box<dyn std::error::Error>> {
    let db = surrealdb::engine::any::connect("mem://").await?;
    db.use_ns("test").use_db("test").await?;
    let sp = SurrealProcessor::new(db);
    let ddl = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../database/schema/orchestration.surql"
    ))?;
    sp.db().query(ddl).await?.check()?;
    Ok(sp)
}

pub fn pos(x: i64, y: i64) -> CanvasUiPosition {
    CanvasUiPosition { x, y }
}

pub async fn canvas(sp: &SurrealProcessor, name: &str) -> Result<CanvasEntity, surrealdb::Error> {
    sp.process(CreateCanvas {
        name: name.to_string(),
        description: String::new(),
    })
    .await
}

pub async fn server(
    sp: &SurrealProcessor,
    canvas: &CanvasEntity,
    name: &str,
) -> Result<ServerEntity, surrealdb::Error> {
    sp.process(CreateServer {
        canvas: canvas.id.clone(),
        name: name.to_string(),
        icon: String::new(),
        comment: String::new(),
        position: pos(0, 0),
        ipv6_resolve: ServerIpv6Resolve::Tolerated,
        log_level: "info".to_string(),
    })
    .await
}

pub async fn server_ip(
    sp: &SurrealProcessor,
    server: &ServerEntity,
    ip: &str,
) -> Result<ServerIpRecordEntity, surrealdb::Error> {
    sp.process(CreateServerIp {
        server: server.id.clone(),
        ip: ip.to_string(),
        country: "jp".to_string(),
    })
    .await
}

pub fn pod_ports() -> Vec<NewPort> {
    vec![
        NewPort {
            kind: PortKind::DeriveListen,
            direction: PortDirection::Output,
            key: "listen".to_string(),
            position: 0,
        },
        NewPort {
            kind: PortKind::DeriveDestination,
            direction: PortDirection::Input,
            key: "destination".to_string(),
            position: 1,
        },
    ]
}

pub fn exit_ports() -> Vec<NewPort> {
    vec![NewPort {
        kind: PortKind::DeriveDestination,
        direction: PortDirection::Output,
        key: "destination".to_string(),
        position: 0,
    }]
}

pub fn entry_ports() -> Vec<NewPort> {
    vec![NewPort {
        kind: PortKind::DeriveListen,
        direction: PortDirection::Input,
        key: "listen".to_string(),
        position: 0,
    }]
}

pub async fn node(
    sp: &SurrealProcessor,
    canvas: &CanvasEntity,
    name: &str,
    spec: NodeSpec,
    ports: Vec<NewPort>,
    revision: i64,
) -> Result<NodeWithPorts, surrealdb::Error> {
    sp.process(CreateNodeRow {
        canvas: canvas.id.clone(),
        name: name.to_string(),
        comment: String::new(),
        spec,
        position: pos(0, 0),
        created_rev: revision,
        replaces: None,
        ports,
    })
    .await
}

pub fn pod_spec(ip: &ServerIpRecordEntity, port: u16) -> NodeSpec {
    NodeSpec::Pod(PodConfig {
        ip: ip.id.clone(),
        port,
    })
}

/// The id of the port with the given key.
pub fn port_of(node: &NodeWithPorts, key: &str) -> orchestration::entities::surreal::port::PortId {
    node.ports
        .iter()
        .find(|p| p.key == key)
        .map(|p| p.id.clone())
        .unwrap_or_else(|| panic!("node has no port {key}"))
}
