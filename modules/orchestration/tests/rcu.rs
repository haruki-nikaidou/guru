//! RCU acceptance: retired rows survive until every server that ran them moved on.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

mod common;

use auth::entities::surreal::account::{AccountId, AccountRole};
use auth::services::identity::{Identity, IdentityKind};
use common::*;
use kanau::processor::Processor;
use orchestration::entities::surreal::canvas::CanvasUiPosition;
use orchestration::entities::surreal::connection::FindEdgeById;
use orchestration::entities::surreal::node::{
    EntryConfig, ExitConfig, FindNodeById, NodeSpec, PodConfig,
};
use orchestration::entities::surreal::revision::{CollectRcuGarbage, ListRetainedRevisions};
use orchestration::entities::surreal::server::{FindServerById, ServerIpv6Resolve};
use orchestration::services::agent::{AckConfig, AgentIdentity, AgentService, RegisterWorker};
use orchestration::services::canvas::CanvasService;
use orchestration::services::edge::{Connect, Disconnect, EdgeService, ForceDisconnect};
use orchestration::services::node::{CreateNode, ForceDeleteNode, NodeService, ReplaceNodeSpec};
use orchestration::services::server::{AddServerIp, CreateServer, ServerService};
use orchestration::services::{OrchestrationError, canvas as canvas_service};
use surrealdb::types::RecordId;
use wakuwaku::surreal::SurrealProcessor;

fn operator() -> Identity {
    Identity {
        account_id: AccountId(RecordId::new("auth_account", "admin")),
        role: AccountRole::Admin,
        kind: IdentityKind::Session,
    }
}

fn machine() -> Identity {
    Identity {
        account_id: AccountId(RecordId::new("auth_account", "admin")),
        role: AccountRole::Maintainer,
        kind: IdentityKind::ApiKey,
    }
}

fn pos0() -> CanvasUiPosition {
    CanvasUiPosition { x: 0, y: 0 }
}

struct World {
    db: SurrealProcessor,
    canvases: CanvasService,
    servers: ServerService,
    nodes: NodeService,
    edges: EdgeService,
    agents: AgentService,
}

async fn world() -> Result<World, Box<dyn std::error::Error>> {
    let db = setup().await?;
    Ok(World {
        canvases: CanvasService { db: db.clone() },
        servers: ServerService { db: db.clone() },
        nodes: NodeService { db: db.clone() },
        edges: EdgeService { db: db.clone() },
        agents: AgentService {
            db: db.clone(),
            hub: Default::default(),
        },
        db,
    })
}

/// Acknowledges a server's current desired revision the way a worker would.
async fn ack_current(
    w: &World,
    server: &orchestration::entities::surreal::server::ServerId,
) -> Result<(), Box<dyn std::error::Error>> {
    w.agents
        .process(RegisterWorker {
            actor: machine(),
            server_id: server.clone(),
            running_revision: 0,
        })
        .await?;
    let row =
        w.db.process(FindServerById { id: server.clone() })
            .await?
            .unwrap();
    w.agents
        .process(AckConfig {
            agent: AgentIdentity {
                server: server.clone(),
                generation: row.refresh_key_generation,
            },
            revision: row.desired_revision,
            error: None,
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn replacing_a_pod_keeps_the_old_row_until_the_server_acks() -> TestResult {
    let w = world().await?;
    let canvas = w
        .canvases
        .process(canvas_service::CreateCanvas {
            actor: operator(),
            name: "prod".to_string(),
            description: String::new(),
        })
        .await?;

    let mut ids = Vec::new();
    for name in ["a", "b"] {
        let server = w
            .servers
            .process(CreateServer {
                actor: operator(),
                canvas: canvas.id.clone(),
                name: name.to_string(),
                icon: String::new(),
                comment: String::new(),
                position: pos0(),
                ipv6_resolve: ServerIpv6Resolve::Tolerated,
                log_level: "info".to_string(),
            })
            .await?;
        ids.push(server.id);
    }
    let (server_a, server_b) = (ids[0].clone(), ids[1].clone());
    let ip = w
        .servers
        .process(AddServerIp {
            actor: operator(),
            server: server_a.clone(),
            ip: "203.0.113.10".to_string(),
            country: "jp".to_string(),
        })
        .await?;

    // Only server A carries a pod.
    let pod = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "edge".to_string(),
            comment: String::new(),
            spec: NodeSpec::Pod(PodConfig {
                ip: ip.id.clone(),
                port: 443,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    let entry = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "entry".to_string(),
            comment: String::new(),
            spec: NodeSpec::Entry(EntryConfig {
                receive_proxy_protocol: None,
                tls: None,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    let exit = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "exit".to_string(),
            comment: String::new(),
            spec: NodeSpec::Exit(ExitConfig {
                destination: "10.0.0.5:8080".to_string(),
                pass_proxy_protocol: None,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    w.edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&pod, "listen"),
            input_port: port_of(&entry, "listen"),
        })
        .await?;
    let edge = w
        .edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&exit, "destination"),
            input_port: port_of(&pod, "destination"),
        })
        .await?;

    let a_before =
        w.db.process(FindServerById {
            id: server_a.clone(),
        })
        .await?
        .unwrap();
    let b_before =
        w.db.process(FindServerById {
            id: server_b.clone(),
        })
        .await?
        .unwrap();
    assert!(
        a_before.desired_revision > 0,
        "server A has a config to roll out"
    );
    assert!(
        b_before.desired_revision > 0,
        "server B is stamped once with its empty config"
    );

    // A is running the current revision; B never registers at all.
    ack_current(&w, &server_a).await?;

    // RCU: change the pod's port.
    let replacement = w
        .nodes
        .process(ReplaceNodeSpec {
            actor: operator(),
            node: pod.node.id.clone(),
            spec: NodeSpec::Pod(PodConfig {
                ip: ip.id.clone(),
                port: 8443,
            }),
            item_count: 0,
        })
        .await?;
    assert_ne!(
        record_key_of(&replacement.node.id),
        record_key_of(&pod.node.id)
    );

    let report = w.db.process(CollectRcuGarbage {}).await?;
    assert_eq!(
        (report.nodes, report.edges),
        (0, 0),
        "server A still runs the previous revision"
    );
    assert!(
        w.db.process(FindNodeById {
            id: pod.node.id.clone()
        })
        .await?
        .is_some()
    );
    assert!(
        w.db.process(FindEdgeById {
            id: edge.id.clone()
        })
        .await?
        .is_some()
    );

    let b_after =
        w.db.process(FindServerById {
            id: server_b.clone(),
        })
        .await?
        .unwrap();
    assert_eq!(
        b_after.desired_revision, b_before.desired_revision,
        "an unrelated server is not re-stamped and never blocks collection"
    );
    let b_retained =
        w.db.process(ListRetainedRevisions {
            server: server_b.clone(),
        })
        .await?;
    assert!(
        b_retained
            .iter()
            .all(|row| row.nodes.is_empty() && row.edges.is_empty()),
        "an empty config pins no rows, so server B can never block collection"
    );

    // Once A applies the new revision the old rows are free.
    ack_current(&w, &server_a).await?;
    let report = w.db.process(CollectRcuGarbage {}).await?;
    assert_eq!(
        (report.nodes, report.edges),
        (1, 2),
        "the replaced pod and both of its edges are freed"
    );
    assert!(
        w.db.process(FindNodeById { id: pod.node.id })
            .await?
            .is_none()
    );
    assert!(w.db.process(FindEdgeById { id: edge.id }).await?.is_none());
    assert!(
        w.db.process(FindNodeById {
            id: replacement.node.id
        })
        .await?
        .is_some()
    );
    Ok(())
}

#[tokio::test]
async fn force_delete_frees_a_row_that_is_still_referenced() -> TestResult {
    let w = world().await?;
    let canvas = w
        .canvases
        .process(canvas_service::CreateCanvas {
            actor: operator(),
            name: "prod".to_string(),
            description: String::new(),
        })
        .await?;
    let server = w
        .servers
        .process(CreateServer {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "a".to_string(),
            icon: String::new(),
            comment: String::new(),
            position: pos0(),
            ipv6_resolve: ServerIpv6Resolve::Tolerated,
            log_level: "info".to_string(),
        })
        .await?;
    let ip = w
        .servers
        .process(AddServerIp {
            actor: operator(),
            server: server.id.clone(),
            ip: "203.0.113.10".to_string(),
            country: "jp".to_string(),
        })
        .await?;
    let pod = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "edge".to_string(),
            comment: String::new(),
            spec: NodeSpec::Pod(PodConfig {
                ip: ip.id.clone(),
                port: 443,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    let entry = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "entry".to_string(),
            comment: String::new(),
            spec: NodeSpec::Entry(EntryConfig {
                receive_proxy_protocol: None,
                tls: None,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    let exit = w
        .nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "exit".to_string(),
            comment: String::new(),
            spec: NodeSpec::Exit(ExitConfig {
                destination: "10.0.0.5:8080".to_string(),
                pass_proxy_protocol: None,
            }),
            position: pos0(),
            item_count: 0,
        })
        .await?;
    w.edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&pod, "listen"),
            input_port: port_of(&entry, "listen"),
        })
        .await?;
    let edge = w
        .edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&exit, "destination"),
            input_port: port_of(&pod, "destination"),
        })
        .await?;
    ack_current(&w, &server.id).await?;

    // A plain disconnect retires the edge; the running revision still pins it.
    w.edges
        .process(Disconnect {
            actor: operator(),
            edge: edge.id.clone(),
        })
        .await?;
    let report = w.db.process(CollectRcuGarbage {}).await?;
    assert_eq!(report.edges, 0);
    assert!(
        w.db.process(FindEdgeById {
            id: edge.id.clone()
        })
        .await?
        .is_some()
    );

    // Force deleting removes it immediately, retained reference or not.
    w.edges
        .process(ForceDisconnect {
            actor: operator(),
            edge: edge.id.clone(),
        })
        .await?;
    assert!(w.db.process(FindEdgeById { id: edge.id }).await?.is_none());

    w.nodes
        .process(ForceDeleteNode {
            actor: operator(),
            node: exit.node.id.clone(),
        })
        .await?;
    assert!(
        w.db.process(FindNodeById { id: exit.node.id })
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn a_worker_credential_cannot_edit_the_workspace() -> TestResult {
    let w = world().await?;
    let err = w
        .canvases
        .process(canvas_service::CreateCanvas {
            actor: machine(),
            name: "prod".to_string(),
            description: String::new(),
        })
        .await
        .expect_err("api keys may not edit the workspace");
    assert!(matches!(err, OrchestrationError::Core(_)), "{err:?}");

    // ...and a human session may not register a worker.
    let canvas = w
        .canvases
        .process(canvas_service::CreateCanvas {
            actor: operator(),
            name: "prod".to_string(),
            description: String::new(),
        })
        .await?;
    let server = w
        .servers
        .process(CreateServer {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "a".to_string(),
            icon: String::new(),
            comment: String::new(),
            position: pos0(),
            ipv6_resolve: ServerIpv6Resolve::Tolerated,
            log_level: "info".to_string(),
        })
        .await?;
    let err = w
        .agents
        .process(RegisterWorker {
            actor: operator(),
            server_id: server.id,
            running_revision: 0,
        })
        .await
        .expect_err("human sessions may not register workers");
    assert!(matches!(err, OrchestrationError::Core(_)), "{err:?}");
    Ok(())
}

fn record_key_of(id: &orchestration::entities::surreal::node::NodeId) -> String {
    orchestration::utils::ids::record_key(&id.0)
}
