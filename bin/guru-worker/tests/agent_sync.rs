//! End-to-end worker ↔ master config sync against an in-process master.
//!
//! The master here is the real thing: `mem://` SurrealDB with both schemas, the
//! real auth services and middleware, the real `WorkerAgent` service and the real
//! revision poller. The worker side is `guru_worker::agent`, driving a real
//! `Supervisor` that binds real sockets.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use auth::entities::surreal::account::{AccountRole, CreateAccount};
use auth::rpc::AuthLayer;
use auth::services::api_key::{ApiKeyService, CreateApiKey};
use auth::services::identity::{Identity, IdentityKind};
use auth::services::session::SessionService;
use auth::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use guru_worker::agent::{self, AgentOptions};
use guru_worker::state;
use guru_worker::supervisor::Supervisor;
use kanau::processor::Processor;
use orchestration::entities::surreal::canvas::CanvasUiPosition;
use orchestration::entities::surreal::node::{EntryConfig, ExitConfig, NodeSpec, PodConfig};
use orchestration::entities::surreal::server::{FindServerById, ServerId, ServerIpv6Resolve};
use orchestration::rpc::WorkerAgentGrpc;
use orchestration::rpc::agent_middleware::AgentLayer;
use orchestration::services::agent::AgentService;
use orchestration::services::canvas::{CanvasService, CreateCanvas};
use orchestration::services::edge::{Connect, EdgeService};
use orchestration::services::node::{CreateNode, NodeService, ReplaceNodeSpec};
use orchestration::services::server::{AddServerIp, CreateServer, ServerService};
use orchestration::services::watch::{self, WatchHub};
use orchestration::utils::ids;
use rpguru_sdk::orchestration_agent::worker_agent_client::WorkerAgentClient;
use rpguru_sdk::orchestration_agent::worker_agent_server::WorkerAgentServer;
use rpguru_sdk::orchestration_agent::{AckConfigRequest, RegisterRequest};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use wakuwaku::surreal::SurrealProcessor;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn operator() -> Identity {
    Identity {
        account_id: auth::entities::surreal::account::AccountId(surrealdb::types::RecordId::new(
            "auth_account",
            "bootstrap",
        )),
        role: AccountRole::Admin,
        kind: IdentityKind::Session,
    }
}

fn pos() -> CanvasUiPosition {
    CanvasUiPosition { x: 0, y: 0 }
}

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

struct Master {
    db: SurrealProcessor,
    hub: WatchHub,
    addr: SocketAddr,
    shutdown: CancellationToken,
}

/// Boots a master process image: schemas, an operator API key, the worker gRPC
/// service and the revision poller.
async fn boot_master() -> Result<(Master, String), Box<dyn std::error::Error>> {
    let db = surrealdb::engine::any::connect("mem://").await?;
    db.use_ns("test").use_db("test").await?;
    let sp = SurrealProcessor::new(db);
    for schema in ["auth", "orchestration"] {
        let ddl = std::fs::read_to_string(format!(
            "{}/../../database/schema/{schema}.surql",
            env!("CARGO_MANIFEST_DIR")
        ))?;
        sp.db().query(ddl).await?.check()?;
    }

    let hasher = Argon2PasswordAlgorithm::default();
    let account = sp
        .process(CreateAccount {
            email: "ops@example.com".to_string(),
            password_hash: hasher.hash_password("hunter2hunter2")?,
            role: AccountRole::Maintainer,
        })
        .await?;
    let api_keys = ApiKeyService { db: sp.clone() };
    let created = api_keys
        .process(CreateApiKey {
            actor: Identity {
                account_id: account.id.clone(),
                role: AccountRole::Maintainer,
                kind: IdentityKind::Session,
            },
            name: "worker".to_string(),
        })
        .await?;

    let hub = WatchHub::default();
    let shutdown = CancellationToken::new();
    let addr = serve(&sp, &hub, &shutdown).await?;
    Ok((
        Master {
            db: sp,
            hub,
            addr,
            shutdown,
        },
        created.secret,
    ))
}

/// Starts one tonic server (a master "process") and returns its address.
async fn serve(
    db: &SurrealProcessor,
    hub: &WatchHub,
    shutdown: &CancellationToken,
) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let hasher = Argon2PasswordAlgorithm::default();
    let sessions = SessionService {
        db: db.clone(),
        hasher,
        config: auth::config::AuthConfig::default(),
    };
    let api_keys = ApiKeyService { db: db.clone() };
    let agents = AgentService {
        db: db.clone(),
        hub: hub.clone(),
    };
    let service = WorkerAgentGrpc {
        agents: agents.clone(),
        db: db.clone(),
        hub: hub.clone(),
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let token = shutdown.clone();
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .layer(AuthLayer::new(sessions, api_keys))
            .layer(AgentLayer::new(agents))
            .add_service(WorkerAgentServer::new(service))
            .serve_with_incoming_shutdown(
                tokio_stream::wrappers::TcpListenerStream::new(listener),
                async move { token.cancelled().await },
            )
            .await;
    });
    let poller_token = shutdown.clone();
    tokio::spawn(watch::run_poller(
        hub.clone(),
        db.clone(),
        Duration::from_millis(50),
        poller_token,
    ));
    Ok(addr)
}

struct Canvas {
    server: ServerId,
    pod: orchestration::entities::surreal::node::NodeId,
    listen: SocketAddr,
    ip: orchestration::entities::surreal::server::ServerIpRecordId,
}

/// One server with a single `pod -> entry` / `exit -> pod` chain on loopback.
async fn build_canvas(db: &SurrealProcessor) -> Result<Canvas, Box<dyn std::error::Error>> {
    let canvases = CanvasService { db: db.clone() };
    let servers = ServerService { db: db.clone() };
    let nodes = NodeService { db: db.clone() };
    let edges = EdgeService { db: db.clone() };

    let canvas = canvases
        .process(CreateCanvas {
            actor: operator(),
            name: "prod".to_string(),
            description: String::new(),
        })
        .await?;
    let server = servers
        .process(CreateServer {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "tokyo".to_string(),
            icon: String::new(),
            comment: String::new(),
            position: pos(),
            ipv6_resolve: ServerIpv6Resolve::Tolerated,
            log_level: "info".to_string(),
        })
        .await?;
    let ip = servers
        .process(AddServerIp {
            actor: operator(),
            server: server.id.clone(),
            ip: "127.0.0.1".to_string(),
            country: "jp".to_string(),
        })
        .await?;
    let port = free_port();
    let pod = nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "edge".to_string(),
            comment: String::new(),
            spec: NodeSpec::Pod(PodConfig {
                ip: ip.id.clone(),
                port,
            }),
            position: pos(),
            item_count: 0,
        })
        .await?;
    let entry = nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "entry".to_string(),
            comment: String::new(),
            spec: NodeSpec::Entry(EntryConfig {
                receive_proxy_protocol: None,
                tls: None,
            }),
            position: pos(),
            item_count: 0,
        })
        .await?;
    let exit = nodes
        .process(CreateNode {
            actor: operator(),
            canvas: canvas.id.clone(),
            name: "exit".to_string(),
            comment: String::new(),
            spec: NodeSpec::Exit(ExitConfig {
                destination: "127.0.0.1:9".to_string(),
                pass_proxy_protocol: None,
            }),
            position: pos(),
            item_count: 0,
        })
        .await?;
    let port_of = |node: &orchestration::entities::surreal::node::NodeWithPorts, key: &str| {
        node.ports
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.id.clone())
            .unwrap()
    };
    edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&pod, "listen"),
            input_port: port_of(&entry, "listen"),
        })
        .await?;
    edges
        .process(Connect {
            actor: operator(),
            output_port: port_of(&exit, "destination"),
            input_port: port_of(&pod, "destination"),
        })
        .await?;

    Ok(Canvas {
        server: server.id,
        pod: pod.node.id,
        listen: format!("127.0.0.1:{port}").parse()?,
        ip: ip.id,
    })
}

/// Waits until `check` holds for the server row, or fails the test.
async fn wait_for<F>(db: &SurrealProcessor, server: &ServerId, what: &str, check: F)
where
    F: Fn(&orchestration::entities::surreal::server::ServerEntity) -> bool,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let row = db
            .process(FindServerById { id: server.clone() })
            .await
            .unwrap()
            .unwrap();
        if check(&row) {
            return;
        }
        if std::time::Instant::now() > deadline {
            panic!(
                "timed out waiting for {what}: desired={} applied={} error={:?}",
                row.desired_revision, row.applied_revision, row.last_apply_error
            );
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn worker_applies_config_and_survives_a_bad_revision() -> TestResult {
    let (master, api_key) = boot_master().await?;
    let canvas = build_canvas(&master.db).await?;
    let state_dir = std::env::temp_dir().join(format!("guru-worker-test-{}", free_port()));
    let _ = std::fs::remove_dir_all(&state_dir);

    let sup = Arc::new(Mutex::new(Supervisor::new()));
    let agent_shutdown = CancellationToken::new();
    let agent_task = tokio::spawn(agent::run(
        AgentOptions {
            master: format!("http://{}", master.addr),
            api_key: api_key.clone(),
            server_id: ids::record_key(&canvas.server.0),
            state_dir: state_dir.clone(),
        },
        sup.clone(),
        agent_shutdown.clone(),
    ));

    // The worker registers, receives the derived config and applies it.
    wait_for(&master.db, &canvas.server, "the first apply", |row| {
        row.desired_revision > 0 && row.applied_revision == row.desired_revision
    })
    .await;
    tokio::net::TcpStream::connect(canvas.listen)
        .await
        .expect("the derived listener is bound");
    let good = state::load(&state_dir).expect("last-known-good was persisted");
    let first_revision = good.revision;
    assert!(good.toml.contains(&canvas.listen.to_string()));

    // A revision that cannot bind is acked with an error and changes nothing.
    let taken = std::net::TcpListener::bind("127.0.0.1:0")?;
    let taken_port = taken.local_addr()?.port();
    let nodes = NodeService {
        db: master.db.clone(),
    };
    nodes
        .process(ReplaceNodeSpec {
            actor: operator(),
            node: canvas.pod.clone(),
            spec: NodeSpec::Pod(PodConfig {
                ip: canvas.ip.clone(),
                port: taken_port,
            }),
            item_count: 0,
        })
        .await?;
    wait_for(&master.db, &canvas.server, "the failed apply", |row| {
        row.last_apply_error.is_some()
    })
    .await;

    let row = master
        .db
        .process(FindServerById {
            id: canvas.server.clone(),
        })
        .await?
        .unwrap();
    assert!(
        row.desired_revision > row.applied_revision,
        "the bad revision was never applied"
    );
    assert_eq!(row.applied_revision, first_revision);
    assert!(
        row.last_apply_error
            .as_deref()
            .unwrap_or_default()
            .contains("edge"),
        "the apply error names the failing forwarding: {:?}",
        row.last_apply_error
    );
    tokio::net::TcpStream::connect(canvas.listen)
        .await
        .expect("the previous listener still serves");
    assert_eq!(
        state::load(&state_dir).unwrap().revision,
        first_revision,
        "last-known-good still points at the config that applied"
    );

    agent_shutdown.cancel();
    let _ = agent_task.await;
    master.shutdown.cancel();
    sup.lock().await.shutdown_all();
    drop(taken);
    let _ = std::fs::remove_dir_all(&state_dir);
    Ok(())
}

#[tokio::test]
async fn registering_again_supersedes_the_previous_refresh_key() -> TestResult {
    let (master, api_key) = boot_master().await?;
    let canvas = build_canvas(&master.db).await?;
    let server_key = ids::record_key(&canvas.server.0);

    let mut client = WorkerAgentClient::connect(format!("http://{}", master.addr)).await?;
    let mut request = tonic::Request::new(RegisterRequest {
        server_id: server_key.clone(),
        running_revision: 0,
    });
    request.metadata_mut().insert("x-api-key", api_key.parse()?);
    let first_key = client.register(request).await?.into_inner().refresh_key;

    // A second registration (a worker restart) rotates the key.
    let mut request = tonic::Request::new(RegisterRequest {
        server_id: server_key.clone(),
        running_revision: 0,
    });
    request.metadata_mut().insert("x-api-key", api_key.parse()?);
    let second_key = client.register(request).await?.into_inner().refresh_key;
    assert_ne!(first_key, second_key);

    let mut stale = tonic::Request::new(AckConfigRequest {
        revision: 1,
        error: None,
    });
    stale
        .metadata_mut()
        .insert("x-refresh-key", first_key.parse()?);
    let status = client
        .ack_config(stale)
        .await
        .expect_err("the superseded key must be rejected");
    assert_eq!(status.code(), tonic::Code::Unauthenticated);

    // A master restart does not invalidate anything: the digest lives in the database.
    master.shutdown.cancel();
    tokio::time::sleep(Duration::from_millis(50)).await;
    let restart = CancellationToken::new();
    let addr = serve(&master.db, &master.hub, &restart).await?;
    let mut client = WorkerAgentClient::connect(format!("http://{addr}")).await?;
    let row = master
        .db
        .process(FindServerById {
            id: canvas.server.clone(),
        })
        .await?
        .unwrap();
    let mut ack = tonic::Request::new(AckConfigRequest {
        revision: row.desired_revision,
        error: None,
    });
    ack.metadata_mut()
        .insert("x-refresh-key", second_key.parse()?);
    client
        .ack_config(ack)
        .await
        .expect("the refresh key survives a master restart");
    let row = master
        .db
        .process(FindServerById { id: canvas.server })
        .await?
        .unwrap();
    assert_eq!(row.applied_revision, row.desired_revision);

    restart.cancel();
    Ok(())
}
