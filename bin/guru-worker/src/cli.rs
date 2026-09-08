use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Clone, Parser)]
#[command(name = "guru-worker", about = "guru data-plane worker")]
pub struct Cli {
    /// Standalone mode: path of the TOML config to load and reload on SIGHUP.
    #[arg(
        short = 'c',
        long,
        env = "GURU_WORKER_CONFIG",
        conflicts_with = "master"
    )]
    pub config: Option<PathBuf>,
    /// Agent mode: `guru-master` worker endpoint, e.g. `http://10.0.0.1:50052`.
    #[arg(long, env = "GURU_MASTER", requires_all = ["api_key", "server"])]
    pub master: Option<String>,
    /// Operator API key used once per session to register with the master.
    #[arg(long, env = "GURU_API_KEY")]
    pub api_key: Option<String>,
    /// `orchestration_server` record key.
    #[arg(long, env = "GURU_SERVER_ID")]
    pub server: Option<String>,
    #[arg(long, env = "GURU_STATE_DIR", default_value = "/var/lib/guru-worker")]
    pub state_dir: PathBuf,
    #[arg(long, env = "GURU_LOG_LEVEL", default_value = "info")]
    pub log_level: String,
}
