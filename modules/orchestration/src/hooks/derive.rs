//! The derivation reactor: turns canvas edits into per-server config snapshots.
//!
//! Every mutating transaction bumps `orchestration_canvas.generation` and publishes
//! [`CanvasDirty`]. This hook derives the whole canvas at the generation it read
//! and commits only while the canvas is still at that generation, so a concurrent
//! edit can never be overwritten by a stale pass — it just loses the race and the
//! pass is redone.
//!
//! The message is a latency hint, not the contract: [`run_sweeper`] re-derives any
//! canvas whose `generation` ran ahead of its `derived_generation`, so a dropped
//! message, a broker outage or a crashed consumer costs at most one sweep interval.

use crate::entities::surreal::canvas::CanvasId;
use crate::entities::surreal::server::ServerId;
use crate::entities::surreal::view::{
    CommitCanvasDerivation, ConfigSnapshot, ListStaleCanvases, LoadCanvasDerivationInput,
    ServerConfigViewEntity, ViewUpdate,
};
use crate::events::CanvasDirty;
use crate::services::converge::converge;
use crate::services::derive::derive_server_config;
use crate::utils::ids::{self, record_key};
use chrono::Utc;
use kanau::processor::Processor;
use std::collections::HashMap;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wakuwaku::amqp::AmqpMessageProcessor;
use wakuwaku::surreal::SurrealProcessor;

/// How many times one pass retries after losing the generation race before it
/// leaves the canvas to the next message or sweep tick.
const MAX_ATTEMPTS: usize = 8;

#[derive(Clone)]
pub struct CanvasDeriver {
    pub db: SurrealProcessor,
}

/// Derives one canvas. The typed input the AMQP consumer and the sweeper share.
pub struct DeriveCanvas {
    pub canvas: CanvasId,
}

impl AmqpMessageProcessor<CanvasDirty> for CanvasDeriver {
    const QUEUE: &'static str = "guru_orchestration_canvas_dirty";
}

impl Processor<CanvasDirty> for CanvasDeriver {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(name = "Hook:CanvasDirty", skip_all, err, fields(canvas = %input.canvas))]
    async fn process(&self, input: CanvasDirty) -> Result<Self::Output, Self::Error> {
        <Self as Processor<DeriveCanvas>>::process(
            self,
            DeriveCanvas {
                canvas: ids::canvas_id(&input.canvas),
            },
        )
        .await
    }
}

impl Processor<DeriveCanvas> for CanvasDeriver {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(name = "Hook:DeriveCanvas", skip_all, err, fields(canvas = ?input.canvas))]
    async fn process(&self, input: DeriveCanvas) -> Result<Self::Output, Self::Error> {
        for _ in 0..MAX_ATTEMPTS {
            let Some(state) = self
                .db
                .process(LoadCanvasDerivationInput {
                    canvas: input.canvas.clone(),
                })
                .await?
            else {
                // The canvas was deleted; its servers keep their last config.
                return Ok(());
            };
            if state.generation == state.derived_generation {
                return Ok(());
            }

            let mut server_of_ip: HashMap<String, ServerId> = HashMap::new();
            for ip in &state.topology.ips {
                server_of_ip.insert(ip.ip.clone(), ip.server.clone());
            }
            let views_by_server: HashMap<String, &ServerConfigViewEntity> = state
                .views
                .iter()
                .map(|view| (record_key(&view.server.0), view))
                .collect();

            let mut updates = Vec::with_capacity(state.topology.servers.len());
            for server in &state.topology.servers {
                let Some(view) = views_by_server.get(&record_key(&server.id.0)) else {
                    tracing::error!(
                        server = %server.name,
                        "server has no config view row; skipping it"
                    );
                    continue;
                };
                updates.push(derive_one(
                    server.id.clone(),
                    view,
                    &state.topology,
                    &state.views,
                    &server_of_ip,
                ));
            }

            if self
                .db
                .process(CommitCanvasDerivation {
                    canvas: input.canvas.clone(),
                    generation: state.generation,
                    updates,
                })
                .await?
            {
                return Ok(());
            }
            // The canvas moved under us: derive the newer state right away rather
            // than waiting for its own message.
        }
        tracing::warn!(canvas = ?input.canvas, "derivation kept losing the generation race");
        Ok(())
    }
}

/// One server's slot in a derivation pass.
///
/// A pod that cannot be derived is reported in `invalid_pods` and costs only its
/// own forwarding; `derive_error` is reserved for a failure of the whole server,
/// which now means only a cross-pod one (two pods claiming a socket) or a broken
/// stored snapshot.
fn derive_one(
    server: ServerId,
    view: &ServerConfigViewEntity,
    topology: &crate::entities::surreal::topology::CanvasTopology,
    views: &[ServerConfigViewEntity],
    server_of_ip: &HashMap<String, ServerId>,
) -> ViewUpdate {
    let failed = |error: String| ViewUpdate {
        server: server.clone(),
        desired: None,
        derive_error: Some(error),
        invalid_pods: Vec::new(),
        waiting_for: Vec::new(),
        clear_failure: false,
    };

    let ideal = match derive_server_config(topology, &server) {
        Ok(ideal) => ideal,
        Err(e) => return failed(e.to_string()),
    };
    let converged = match converge(ideal, view, views, server_of_ip) {
        Ok(converged) => converged,
        Err(e) => return failed(e.to_string()),
    };
    if let Err(e) = converged.config.validate() {
        return failed(e.to_string());
    }
    let toml = match converged.config.to_toml_string() {
        Ok(toml) => toml,
        Err(e) => return failed(e.to_string()),
    };

    if view.desired.as_ref().map(|s| s.toml.as_str()) == Some(toml.as_str()) {
        // Byte-identical: no new revision, so the worker is never restarted for an
        // edit that does not concern it. The pod report still has to land: a pod
        // may have broken (or been fixed) without changing the served config.
        return ViewUpdate {
            server,
            desired: None,
            derive_error: None,
            invalid_pods: converged.invalid,
            waiting_for: converged.waiting_for,
            clear_failure: false,
        };
    }
    let revision = view
        .desired
        .as_ref()
        .map(|s| s.revision)
        .unwrap_or(0)
        .saturating_add(1);
    ViewUpdate {
        server,
        desired: Some(ConfigSnapshot {
            revision,
            toml,
            created_at: Utc::now(),
            forwardings: converged.forwardings,
        }),
        derive_error: None,
        invalid_pods: converged.invalid,
        waiting_for: converged.waiting_for,
        clear_failure: true,
    }
}

/// Re-derives canvases whose edits outran their derivation, until `shutdown`.
///
/// This is what makes the AMQP path optional: correctness lives in the generation
/// counters, the message only shortens the delay.
pub async fn run_sweeper(deriver: CanvasDeriver, interval: Duration, shutdown: CancellationToken) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = ticker.tick() => {}
        }
        let canvases = match deriver.db.process(ListStaleCanvases).await {
            Ok(canvases) => canvases,
            Err(e) => {
                tracing::error!(error = %e, "listing stale canvases failed");
                continue;
            }
        };
        for canvas in canvases {
            if let Err(e) = deriver.process(DeriveCanvas { canvas }).await {
                tracing::error!(error = %e, "sweeping a canvas failed");
            }
        }
    }
}
