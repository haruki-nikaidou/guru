#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

//! The guru data-plane worker.
//!
//! The binary is a thin wrapper around [`run`]; everything else lives here so that
//! integration tests can drive the supervisor and the control-plane agent directly.

pub mod agent;
pub mod cli;
pub mod listener;
pub mod pipe;
pub mod prepared;
pub mod resolver;
pub mod state;
pub mod supervisor;
pub mod tls;

pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Default config path used by standalone mode when `--config` is absent.
pub const DEFAULT_CONFIG_PATH: &str = "/etc/guru-worker/config.toml";

pub fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::new(level);
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Runs the worker until SIGTERM/SIGINT.
///
/// Standalone mode (no `--master`) loads a config file and reloads it on SIGHUP.
/// Agent mode registers with `guru-master` and applies every config revision it streams.
pub async fn run(cli: cli::Cli) -> Result<(), BoxError> {
    let _ = quinn::rustls::crypto::ring::default_provider().install_default();
    match cli.master.clone() {
        None => run_standalone(cli).await,
        Some(master) => run_agent(cli, master).await,
    }
}

async fn run_standalone(cli: cli::Cli) -> Result<(), BoxError> {
    let path = cli
        .config
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_CONFIG_PATH));
    let cfg = guru_worker_config::Config::load(&path)?;
    init_tracing(&cfg.log.level);
    tracing::info!(path = %path.display(), "loaded config");

    let mut sup = supervisor::Supervisor::new();
    sup.apply(&cfg)?;

    use tokio::signal::unix::{SignalKind, signal};
    let mut hup = signal(SignalKind::hangup())?;
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    loop {
        tokio::select! {
            _ = hup.recv() => {
                match guru_worker_config::Config::load(&path) {
                    Ok(c) => {
                        if let Err(e) = sup.apply(&c) {
                            tracing::error!(error = %e, "reload apply failed; keeping running config");
                        } else {
                            tracing::info!("config reloaded");
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "reload failed; keeping running config"),
                }
            }
            _ = term.recv() => break,
            _ = int.recv() => break,
        }
    }
    tracing::info!("shutting down");
    sup.shutdown_all();
    Ok(())
}

async fn run_agent(cli: cli::Cli, master: String) -> Result<(), BoxError> {
    let (Some(api_key), Some(server_id)) = (cli.api_key.clone(), cli.server.clone()) else {
        return Err("agent mode requires --api-key and --server".into());
    };
    init_tracing(&cli.log_level);

    let sup = std::sync::Arc::new(tokio::sync::Mutex::new(supervisor::Supervisor::new()));
    if let Some(good) = state::load(&cli.state_dir) {
        match guru_worker_config::Config::from_toml_str(&good.toml) {
            Ok(cfg) => match sup.lock().await.apply(&cfg) {
                Ok(()) => {
                    tracing::info!(revision = good.revision, "applied last-known-good config")
                }
                Err(e) => tracing::error!(error = %e, "last-known-good config failed to apply"),
            },
            Err(e) => tracing::error!(error = %e, "last-known-good config is invalid"),
        }
    }

    let shutdown = tokio_util::sync::CancellationToken::new();
    let agent = tokio::spawn(agent::run(
        agent::AgentOptions {
            master,
            api_key,
            server_id,
            state_dir: cli.state_dir.clone(),
        },
        sup.clone(),
        shutdown.clone(),
    ));

    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = term.recv() => {}
        _ = int.recv() => {}
    }
    tracing::info!("shutting down");
    shutdown.cancel();
    let _ = agent.await;
    sup.lock().await.shutdown_all();
    Ok(())
}
