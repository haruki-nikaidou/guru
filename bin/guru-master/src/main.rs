//! # `guru-master`
//!
//! The control plane. One binary, three run modes selected with `--mode`:
//!
//! - `dashboard_grpc` — the operator API (`Auth` + `Orchestration`),
//! - `workers_grpc` — the worker API (`WorkerAgent`) plus the revision poller,
//! - `cron` — periodic jobs, currently the RCU garbage collector.
//!
//! Wiring only: every rule lives in the modules under `modules/`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use auth::config::AuthConfig;
use auth::rpc::{AuthGrpc, AuthLayer};
use auth::services::account::AccountService;
use auth::services::api_key::ApiKeyService;
use auth::services::session::SessionService;
use auth::utils::password::Argon2PasswordAlgorithm;
use clap::Parser;
use kanau::processor::Processor;
use orchestration::hooks::gc::{GcTick, RcuGarbageCollector};
use orchestration::rpc::agent_middleware::AgentLayer;
use orchestration::rpc::{OrchestrationGrpc, WorkerAgentGrpc};
use orchestration::services::agent::AgentService;
use orchestration::services::canvas::CanvasService;
use orchestration::services::edge::EdgeService;
use orchestration::services::node::NodeService;
use orchestration::services::rollout::RolloutService;
use orchestration::services::server::ServerService;
use orchestration::services::watch::{self, WatchHub};
use rpguru_sdk::auth::auth_server::AuthServer;
use rpguru_sdk::orchestration::orchestration_server::OrchestrationServer;
use rpguru_sdk::orchestration_agent::worker_agent_server::WorkerAgentServer;
use std::net::SocketAddr;
use std::time::Duration;
use surrealdb::opt::auth::Root;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use tracing_subscriber::EnvFilter;
use wakuwaku::surreal::SurrealProcessor;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum WorkerMode {
    #[value(name = "dashboard_grpc")]
    DashboardGrpc,
    #[value(name = "workers_grpc")]
    WorkersGrpc,
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
    #[arg(long, env = "GURU_GC_INTERVAL_SECS", default_value = "60")]
    gc_interval_secs: u64,
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
            let accounts = AccountService {
                db: db.clone(),
                hasher,
            };
            let orchestration = OrchestrationGrpc {
                canvases: CanvasService { db: db.clone() },
                servers: ServerService { db: db.clone() },
                nodes: NodeService { db: db.clone() },
                edges: EdgeService { db: db.clone() },
                rollout: RolloutService { db: db.clone() },
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
            let hub = WatchHub::default();
            let agents = AgentService {
                db: db.clone(),
                hub: hub.clone(),
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
            };
            tracing::info!(addr = %cli.workers_addr, "serving worker API");
            // `AuthLayer` is required here too: `Register` authenticates with an
            // operator API key before any refresh key exists.
            Server::builder()
                .layer(AuthLayer::new(sessions, api_keys))
                .layer(AgentLayer::new(agents))
                .add_service(WorkerAgentServer::new(workers))
                .serve_with_shutdown(cli.workers_addr, shutdown())
                .await?;
            token.cancel();
            let _ = poller.await;
        }
        WorkerMode::Cron => {
            let gc = RcuGarbageCollector { db };
            let mut ticker = tokio::time::interval(Duration::from_secs(cli.gc_interval_secs));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            tracing::info!(interval_secs = cli.gc_interval_secs, "running cron worker");
            let shutdown = std::pin::pin!(shutdown());
            let mut shutdown = shutdown;
            loop {
                tokio::select! {
                    _ = &mut shutdown => break,
                    _ = ticker.tick() => {
                        if let Err(e) = gc.process(GcTick).await {
                            tracing::error!(error = %e, "rcu gc failed");
                        }
                    }
                }
            }
        }
    }
    Ok(())
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
