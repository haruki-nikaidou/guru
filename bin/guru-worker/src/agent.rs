//! Control-plane agent: registers with `guru-master`, streams config revisions and
//! acknowledges each one.
//!
//! The dynamic refresh key handed out by `Register` lives in memory only, so a worker
//! restart always produces a fresh registration and the master can tell it was down.

use crate::BoxError;
use crate::state::{self, LastKnownGood};
use crate::supervisor::Supervisor;
use rpguru_sdk::orchestration_agent::{
    AckConfigRequest, RegisterRequest, WatchConfigRequest, worker_agent_client::WorkerAgentClient,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub struct AgentOptions {
    pub master: String,
    pub api_key: String,
    pub server_id: String,
    pub state_dir: PathBuf,
}

const BACKOFF_START: Duration = Duration::from_secs(1);
const BACKOFF_CAP: Duration = Duration::from_secs(30);

/// Keeps a session with the master alive, reconnecting with capped exponential backoff.
pub async fn run(
    opts: AgentOptions,
    sup: Arc<Mutex<Supervisor>>,
    shutdown: CancellationToken,
) -> Result<(), BoxError> {
    let mut backoff = BACKOFF_START;
    loop {
        if shutdown.is_cancelled() {
            return Ok(());
        }
        match session(&opts, &sup, &shutdown, &mut backoff).await {
            Ok(()) => return Ok(()),
            Err(e) => tracing::error!(error = %e, "agent session ended"),
        }
        tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = std::cmp::min(backoff.saturating_mul(2), BACKOFF_CAP);
    }
}

/// One connection: register, watch, apply, ack. Returns `Ok(())` only on shutdown.
async fn session(
    opts: &AgentOptions,
    sup: &Arc<Mutex<Supervisor>>,
    shutdown: &CancellationToken,
    backoff: &mut Duration,
) -> Result<(), BoxError> {
    let mut client = WorkerAgentClient::connect(opts.master.clone()).await?;

    let running_revision = state::load(&opts.state_dir)
        .map(|g| g.revision)
        .unwrap_or(0);
    let mut register = tonic::Request::new(RegisterRequest {
        server_id: opts.server_id.clone(),
        running_revision,
    });
    register
        .metadata_mut()
        .insert("x-api-key", opts.api_key.parse()?);
    let refresh_key = client.register(register).await?.into_inner().refresh_key;
    *backoff = BACKOFF_START;
    tracing::info!(
        server = %opts.server_id,
        running_revision,
        "registered with master"
    );

    let mut watch = tonic::Request::new(WatchConfigRequest {});
    watch
        .metadata_mut()
        .insert("x-refresh-key", refresh_key.parse()?);
    let mut stream = client.watch_config(watch).await?.into_inner();

    loop {
        let message = tokio::select! {
            _ = shutdown.cancelled() => return Ok(()),
            message = stream.message() => message?,
        };
        let Some(revision) = message else {
            return Err("config stream ended".into());
        };

        let error = match guru_worker_config::Config::from_toml_str(&revision.toml) {
            Err(e) => Some(format!("config: {e}")),
            Ok(cfg) => match sup.lock().await.apply(&cfg) {
                Err(e) => Some(e.to_string()),
                Ok(()) => {
                    state::store(
                        &opts.state_dir,
                        &LastKnownGood {
                            revision: revision.revision,
                            toml: revision.toml.clone(),
                        },
                    )?;
                    tracing::info!(revision = revision.revision, "applied config revision");
                    None
                }
            },
        };
        if let Some(error) = &error {
            tracing::error!(
                revision = revision.revision,
                error,
                "config revision rejected"
            );
        }

        let mut ack = tonic::Request::new(AckConfigRequest {
            revision: revision.revision,
            error,
        });
        ack.metadata_mut()
            .insert("x-refresh-key", refresh_key.parse()?);
        client.ack_config(ack).await?;
    }
}
