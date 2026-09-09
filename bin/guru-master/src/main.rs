//! # `guru-master`
//!
//! The control plane. One binary, four run modes selected with `--mode`:
//!
//! - `dashboard_grpc` — the operator API (`Auth` + `Orchestration`),
//! - `workers_grpc` — the worker API (`WorkerAgent`) plus the config-view poller,
//! - `consumer` — the AMQP derivation hook,
//! - `cron` — periodic jobs, currently the stale-canvas derivation sweep.
//!
//! Wiring only: every rule lives in the modules under `modules/`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use amqprs::callbacks::{DefaultChannelCallback, DefaultConnectionCallback};
use amqprs::channel::BasicQosArguments;
use amqprs::connection::{Connection, OpenConnectionArguments};
use auth::config::AuthConfig;
use auth::rpc::{AuthGrpc, AuthLayer};
use auth::services::account::AccountService;
use auth::services::api_key::ApiKeyService;
use auth::services::session::SessionService;
use auth::utils::password::Argon2PasswordAlgorithm;
use clap::Parser;
use orchestration::events::CanvasDirty;
use orchestration::hooks::derive::{self, CanvasDeriver};
use orchestration::rpc::agent_middleware::AgentLayer;
use orchestration::rpc::{OrchestrationGrpc, WorkerAgentGrpc};
use orchestration::services::agent::AgentService;
use orchestration::services::canvas::CanvasService;
use orchestration::services::edge::EdgeService;
use orchestration::services::node::NodeService;
use orchestration::services::rollout::{DirtyNotifier, RolloutService};
use orchestration::services::server::ServerService;
use orchestration::services::watch::{self, SessionLease, WatchHub};
use rpguru_sdk::auth::auth_server::AuthServer;
use rpguru_sdk::orchestration::orchestration_server::OrchestrationServer;
use rpguru_sdk::orchestration_agent::worker_agent_server::WorkerAgentServer;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use surrealdb::opt::auth::Root;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use wakuwaku::amqp::{AmqpMessageProcessor, AmqpPool, AmqpRouting, setup_consumer};
use wakuwaku::surreal::SurrealProcessor;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum WorkerMode {
    #[value(name = "dashboard_grpc")]
    DashboardGrpc,
    #[value(name = "workers_grpc")]
    WorkersGrpc,
    #[value(name = "consumer")]
    Consumer,
    #[value(name = "cron")]
    Cron,
}

#[derive(Debug, Parser)]
#[command(name = "guru-master", about = "guru control plane")]
struct Cli {
    #[arg(
        long,
        env = "GURU_WORKER_MODE",
        value_enum,
        default_value = "dashboard_grpc"
    )]
    mode: WorkerMode,
    #[arg(
        long,
        env = "GURU_DASHBOARD_GRPC_ADDR",
        default_value = "0.0.0.0:50051"
    )]
    dashboard_addr: SocketAddr,
    #[arg(long, env = "GURU_WORKERS_GRPC_ADDR", default_value = "0.0.0.0:50052")]
    workers_addr: SocketAddr,
    #[arg(long, env = "SURREALDB_HOST", default_value = "ws://127.0.0.1:8000")]
    address: String,
    #[arg(long, env = "SURREALDB_USER", default_value = "root")]
    username: String,
    #[arg(long, env = "SURREALDB_PASSWORD", default_value = "root")]
    password: String,
    #[arg(long, env = "SURREALDB_NAMESPACE")]
    namespace: String,
    #[arg(long, env = "SURREALDB_NAME")]
    database: String,
    #[arg(long, env = "AMQP_URI")]
    amqp_uri: Option<String>,
    #[arg(long, env = "GURU_SWEEP_INTERVAL_SECS", default_value = "30")]
    sweep_interval_secs: u64,
    #[arg(long, env = "GURU_WATCH_POLL_MS", default_value = "1000")]
    watch_poll_ms: u64,
    #[arg(long, env = "GURU_LOG_LEVEL", default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(&cli.log_level))
        .init();

    let db = surrealdb::engine::any::connect(&cli.address).await?;
    db.signin(Root {
        username: cli.username.clone(),
        password: cli.password.clone(),
    })
    .await?;
    db.use_ns(&cli.namespace).use_db(&cli.database).await?;
    let db = SurrealProcessor::new(db);

    let hasher = Argon2PasswordAlgorithm::default();
    let sessions = SessionService {
        db: db.clone(),
        hasher: hasher.clone(),
        config: AuthConfig::default(),
    };
    let api_keys = ApiKeyService { db: db.clone() };

    match cli.mode {
        WorkerMode::DashboardGrpc => {
            let (_connection, notifier) = notifier(cli.amqp_uri.as_deref()).await?;
            let accounts = AccountService {
                db: db.clone(),
                hasher,
            };
            let orchestration = OrchestrationGrpc {
                canvases: CanvasService {
                    db: db.clone(),
                    notifier: notifier.clone(),
                },
                servers: ServerService {
                    db: db.clone(),
                    notifier: notifier.clone(),
                },
                nodes: NodeService {
                    db: db.clone(),
                    notifier: notifier.clone(),
                },
                edges: EdgeService {
                    db: db.clone(),
                    notifier: notifier.clone(),
                },
                rollout: RolloutService {
                    db: db.clone(),
                    notifier,
                },
            };
            let auth = AuthGrpc {
                accounts,
                sessions: sessions.clone(),
                api_keys: api_keys.clone(),
            };
            tracing::info!(addr = %cli.dashboard_addr, "serving operator API");
            Server::builder()
                .layer(AuthLayer::new(sessions, api_keys))
                .add_service(AuthServer::new(auth))
                .add_service(OrchestrationServer::new(orchestration))
                .serve_with_shutdown(cli.dashboard_addr, shutdown())
                .await?;
        }
        WorkerMode::WorkersGrpc => {
            let (_connection, notifier) = notifier(cli.amqp_uri.as_deref()).await?;
            let hub = WatchHub::default();
            let lease = SessionLease::default();
            let agents = AgentService {
                db: db.clone(),
                hub: hub.clone(),
                lease,
                notifier,
            };
            let token = CancellationToken::new();
            let poller = tokio::spawn(watch::run_poller(
                hub.clone(),
                db.clone(),
                Duration::from_millis(cli.watch_poll_ms),
                token.clone(),
            ));
            let workers = WorkerAgentGrpc {
                agents: agents.clone(),
                db: db.clone(),
                hub,
                lease,
            };
            tracing::info!(addr = %cli.workers_addr, "serving worker API");
            // `AuthLayer` is required here too: `Register` authenticates with an
            // operator API key before any refresh key exists.
            //
            // Keepalive is load-bearing: a worker that dies without closing its TCP
            // connection would otherwise keep renewing its session lease and lock
            // its replacement out of `Register`.
            Server::builder()
                .http2_keepalive_interval(Some(lease.heartbeat))
                .http2_keepalive_timeout(Some(lease.heartbeat))
                .layer(AuthLayer::new(sessions, api_keys))
                .layer(AgentLayer::new(agents))
                .add_service(WorkerAgentServer::new(workers))
                .serve_with_shutdown(cli.workers_addr, shutdown())
                .await?;
            token.cancel();
            let _ = poller.await;
        }
        WorkerMode::Consumer => {
            let uri = cli
                .amqp_uri
                .as_deref()
                .ok_or("consumer mode needs AMQP_URI")?;
            let (connection, pool) = amqp_pool(uri).await?;
            let channel = CanvasDeriver::ensure_queue(&pool).await?;
            channel
                .register_callback(DefaultChannelCallback)
                .await
                .map_err(|e| format!("registering the channel callback failed: {e}"))?;
            // A bounded prefetch keeps one consumer from hoarding the whole backlog
            // while its peers idle; derivation is a database transaction, not a
            // cheap ack.
            channel
                .basic_qos(BasicQosArguments::new(0, 8, false))
                .await
                .map_err(|e| format!("setting the consumer prefetch failed: {e}"))?;
            setup_consumer::<CanvasDirty, CanvasDeriver>(&channel, Arc::new(CanvasDeriver { db }))
                .await
                .map_err(|e| format!("binding the consumer failed: {e}"))?;
            tracing::info!(queue = CanvasDeriver::QUEUE, "consuming canvas edits");
            shutdown().await;
            drop(channel);
            connection
                .close()
                .await
                .map_err(|e| format!("closing the AMQP connection failed: {e}"))?;
        }
        WorkerMode::Cron => {
            let token = CancellationToken::new();
            let sweeper = tokio::spawn(derive::run_sweeper(
                CanvasDeriver { db },
                Duration::from_secs(cli.sweep_interval_secs),
                token.clone(),
            ));
            tracing::info!(
                interval_secs = cli.sweep_interval_secs,
                "running cron worker"
            );
            shutdown().await;
            token.cancel();
            let _ = sweeper.await;
        }
    }
    Ok(())
}

/// Opens an AMQP connection and its channel pool, declaring the exchange.
///
/// The connection is returned so the caller can keep it alive: dropping it closes
/// every pooled channel.
async fn amqp_pool(uri: &str) -> Result<(Connection, AmqpPool), Box<dyn std::error::Error>> {
    let args = OpenConnectionArguments::try_from(uri)
        .map_err(|e| format!("parsing AMQP_URI failed: {e}"))?;
    let connection = Connection::open(&args)
        .await
        .map_err(|e| format!("connecting to AMQP failed: {e}"))?;
    connection
        .register_callback(DefaultConnectionCallback)
        .await
        .map_err(|e| format!("registering the AMQP callback failed: {e}"))?;
    let pool = AmqpPool::connect(connection.clone()).await;
    CanvasDirty::ensure_exchange(&pool)
        .await
        .map_err(|e| format!("declaring the orchestration exchange failed: {e}"))?;
    Ok((connection, pool))
}

/// The dirty-canvas notifier for a serving mode. Without a broker the cron sweep
/// is the only derivation trigger, which is correct but slower.
async fn notifier(
    uri: Option<&str>,
) -> Result<(Option<Connection>, DirtyNotifier), Box<dyn std::error::Error>> {
    let Some(uri) = uri else {
        tracing::warn!("AMQP_URI unset: relying on the cron sweep for derivation");
        return Ok((None, DirtyNotifier::default()));
    };
    let (connection, pool) = amqp_pool(uri).await?;
    Ok((Some(connection), DirtyNotifier { amqp: Some(pool) }))
}

/// Completes on SIGTERM or SIGINT.
async fn shutdown() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "cannot listen for SIGTERM");
            return;
        }
    };
    let mut int = match signal(SignalKind::interrupt()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "cannot listen for SIGINT");
            return;
        }
    };
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    tracing::info!("shutting down");
}
