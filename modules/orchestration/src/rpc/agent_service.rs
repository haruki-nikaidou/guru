//! The `WorkerAgent` gRPC service: registration, config streaming, acknowledgement.

use crate::entities::surreal::revision::FindServerConfigRevision;
use crate::entities::surreal::server::{
    ClaimServerWatchSession, FindServerById, ReleaseServerWatchSession, RenewServerWatchSession,
    ServerEntity, ServerId,
};
use crate::rpc::agent_middleware::agent_from_request;
use crate::services::agent::{AckConfig, AgentService, RegisterWorker};
use crate::services::watch::{AgentSignal, SessionLease, WatchFence, WatchHub};
use crate::utils::ids;
use kanau::processor::Processor;
use rpguru_sdk::orchestration_agent as pb;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use wakuwaku::surreal::SurrealProcessor;

const STREAM_CAPACITY: usize = 4;

#[derive(Clone)]
pub struct WorkerAgentGrpc {
    pub agents: AgentService,
    pub db: SurrealProcessor,
    pub hub: WatchHub,
    pub lease: SessionLease,
}

impl WorkerAgentGrpc {
    /// The stored TOML of one revision, if it is still retained.
    ///
    /// A database failure is *not* a missing revision: swallowing it would make the
    /// stream skip a revision the hub has already marked as delivered, leaving the
    /// worker on stale config until the next unrelated edit.
    async fn revision_toml(
        &self,
        server: &ServerId,
        revision: i64,
    ) -> Result<Option<String>, Status> {
        self.db
            .process(FindServerConfigRevision {
                server: server.clone(),
                revision,
            })
            .await
            .map(|row| row.map(|row| row.toml))
            .map_err(|e| Status::internal(e.to_string()))
    }

    async fn server_row(&self, server: &ServerId) -> Result<ServerEntity, Status> {
        self.db
            .process(FindServerById { id: server.clone() })
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Not found"))
    }

    /// What to send for `revision`. A revision that was pruned between the signal
    /// and the read is replaced by whatever the server should be running now;
    /// `Ok(None)` means there is nothing to send yet.
    async fn resolve(
        &self,
        server: &ServerId,
        revision: i64,
    ) -> Result<Option<pb::ConfigRevision>, Status> {
        if revision > 0
            && let Some(toml) = self.revision_toml(server, revision).await?
        {
            return Ok(Some(pb::ConfigRevision { revision, toml }));
        }
        let row = self.server_row(server).await?;
        if row.desired_revision == 0 || row.desired_revision == revision {
            return Ok(None);
        }
        Ok(self
            .revision_toml(server, row.desired_revision)
            .await?
            .map(|toml| pb::ConfigRevision {
                revision: row.desired_revision,
                toml,
            }))
    }

    /// Extends this session's lease. `false` means the fence moved on.
    async fn renew(&self, server: &ServerId, fence: WatchFence) -> Result<bool, Status> {
        let now = chrono::Utc::now();
        self.db
            .process(RenewServerWatchSession {
                server: server.clone(),
                generation: fence.generation,
                epoch: fence.epoch,
                now,
                lease_until: self.lease.until(now),
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))
    }

    /// Best effort: a lease that outlives its stream only delays the next
    /// registration until it lapses.
    async fn release(&self, server: &ServerId, fence: WatchFence) {
        if let Err(e) = self
            .db
            .process(ReleaseServerWatchSession {
                server: server.clone(),
                generation: fence.generation,
                epoch: fence.epoch,
            })
            .await
        {
            tracing::warn!(error = %e, "releasing the watch session lease failed");
        }
    }
}

#[tonic::async_trait]
impl pb::worker_agent_server::WorkerAgent for WorkerAgentGrpc {
    async fn register(
        &self,
        request: Request<pb::RegisterRequest>,
    ) -> Result<Response<pb::RegisterReply>, Status> {
        let actor = auth::rpc::middleware::from_request(&request)?;
        let input = request.into_inner();
        let refresh_key = self
            .agents
            .process(RegisterWorker {
                actor,
                server_id: ids::server_id(&input.server_id),
                running_revision: input.running_revision,
            })
            .await?;
        Ok(Response::new(pb::RegisterReply { refresh_key }))
    }

    type WatchConfigStream = ReceiverStream<Result<pb::ConfigRevision, Status>>;

    async fn watch_config(
        &self,
        request: Request<pb::WatchConfigRequest>,
    ) -> Result<Response<Self::WatchConfigStream>, Status> {
        let agent = agent_from_request(&request)?;
        // Claim the server's single watch session. The claim is conditional on the
        // generation still being current, so a request that authenticated just
        // before a registration rotated the key cannot open a stream afterwards.
        let now = chrono::Utc::now();
        let server = self
            .db
            .process(ClaimServerWatchSession {
                server: agent.server.clone(),
                generation: agent.generation,
                now,
                lease_until: self.lease.until(now),
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::unauthenticated("refresh key superseded"))?;
        let fence = WatchFence {
            generation: server.refresh_key_generation,
            epoch: server.watch_epoch,
        };

        let server_key = ids::record_key(&server.id.0);
        // Seeded with what this session is about to be sent, so the next poll does
        // not re-broadcast the revision it already has.
        let Some(subscription) = self
            .hub
            .subscribe(&server_key, fence, server.desired_revision)
        else {
            // Our claim already lost to a newer one; the release is a no-op unless we
            // are somehow still the row's owner.
            self.release(&server.id, fence).await;
            return Err(Status::aborted("a newer watch session took over"));
        };
        let (tx, rx) = mpsc::channel(STREAM_CAPACITY);

        let this = self.clone();
        let server_id = server.id.clone();
        // Everything past the claim runs in the task, which always releases the
        // lease on its way out: an early return here would hold the server hostage
        // for a full lease period.
        tokio::spawn(async move {
            let mut subscription = subscription;
            let mut heartbeat = tokio::time::interval(this.lease.heartbeat);
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            heartbeat.tick().await; // the claim already took the lease
            // `Err` ends the stream: the worker reconnects with backoff and starts
            // from the row again, so a failed read is never a silently lost revision.
            let ended: Result<(), Status> = async {
                // Send what the server should be running right now. The hub may know
                // a newer revision than the claimed row did — that broadcast is
                // already spent, so it has to be picked up here or never.
                let known = server.desired_revision.max(subscription.last_revision);
                if let Some(message) = this.resolve(&server_id, known).await?
                    && tx.send(Ok(message)).await.is_err()
                {
                    return Ok(());
                }
                loop {
                    let signal = tokio::select! {
                        // A worker that vanishes quietly must hand its lease back;
                        // this task is detached, so nothing else would notice.
                        _ = tx.closed() => return Ok(()),
                        _ = heartbeat.tick() => {
                            // Holding the lease is what keeps a second worker from
                            // registering; losing it means we are no longer the owner.
                            if !this.renew(&server_id, fence).await? {
                                return Err(Status::aborted("watch session lease lost"));
                            }
                            continue;
                        }
                        signal = subscription.rx.recv() => signal,
                    };
                    match signal {
                        Ok(AgentSignal::Revision(revision)) => {
                            if let Some(message) = this.resolve(&server_id, revision).await?
                                && tx.send(Ok(message)).await.is_err()
                            {
                                return Ok(());
                            }
                        }
                        Ok(AgentSignal::Fenced(current)) if current != fence => {
                            return Err(fenced_status(fence, current));
                        }
                        Ok(AgentSignal::Fenced(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            // The dropped messages may have contained our own fence
                            // signal, so re-check it before trusting the stream.
                            let row = this.server_row(&server_id).await?;
                            let current = WatchFence {
                                generation: row.refresh_key_generation,
                                epoch: row.watch_epoch,
                            };
                            if current != fence {
                                return Err(fenced_status(fence, current));
                            }
                            if let Some(message) =
                                this.resolve(&server_id, row.desired_revision).await?
                                && tx.send(Ok(message)).await.is_err()
                            {
                                return Ok(());
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
            }
            .await;
            // A stream that ends for any reason hands the server back immediately, so
            // a restarting worker does not have to wait the lease out.
            this.release(&server_id, fence).await;
            if let Err(status) = ended {
                let _ = tx.send(Err(status)).await;
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn ack_config(
        &self,
        request: Request<pb::AckConfigRequest>,
    ) -> Result<Response<pb::AckConfigReply>, Status> {
        let agent = agent_from_request(&request)?;
        let input = request.into_inner();
        self.agents
            .process(AckConfig {
                agent,
                revision: input.revision,
                error: input.error,
            })
            .await?;
        Ok(Response::new(pb::AckConfigReply {}))
    }
}

/// Why a stream lost the fence: a new registration (the worker restarted, or an
/// impostor registered) or a newer stream for the same generation.
fn fenced_status(mine: WatchFence, current: WatchFence) -> Status {
    if current.generation != mine.generation {
        Status::unauthenticated("refresh key superseded")
    } else {
        Status::aborted("a newer watch session took over")
    }
}
