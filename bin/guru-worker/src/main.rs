#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

pub(crate) mod config;
pub(crate) mod listener;
pub(crate) mod pipe;
pub(crate) mod prepared;
pub(crate) mod resolver;
pub(crate) mod supervisor;
pub(crate) mod tls;

pub(crate) type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

fn main() -> std::process::ExitCode {
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("runtime: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match rt.block_on(run()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fatal: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), BoxError> {
    let _ = quinn::rustls::crypto::ring::default_provider().install_default();
    let path = parse_config_path();
    let cfg = config::Config::load(&path)?;
    init_tracing(&cfg.log.level);
    tracing::info!(path = %path.display(), "loaded config");

    let mut sup = supervisor::Supervisor::new();
    sup.apply(&cfg, true).await?;

    use tokio::signal::unix::{SignalKind, signal};
    let mut hup = signal(SignalKind::hangup())?;
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    loop {
        tokio::select! {
            _ = hup.recv() => {
                match config::Config::load(&path) {
                    Ok(c) => {
                        if let Err(e) = sup.apply(&c, false).await {
                            tracing::error!(error = %e, "reload apply error");
                        }
                        tracing::info!("config reloaded");
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

fn parse_config_path() -> std::path::PathBuf {
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if (arg == "-c" || arg == "--config")
            && let Some(p) = args.next()
        {
            return std::path::PathBuf::from(p);
        }
    }
    std::path::PathBuf::from("/etc/guru-worker/config.toml")
}

fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::new(level);
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
