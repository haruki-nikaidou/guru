//! The `WorkerAgent` gRPC service: registration, config streaming, acknowledgement.

use crate::entities::surreal::revision::FindServerConfigRevision;
use crate::entities::surreal::server::FindServerById;
use crate::rpc::agent_middleware::agent_from_request;
use crate::services::agent::{AckConfig, AgentService, RegisterWorker};
use crate::services::watch::{AgentSignal, WatchHub};
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
}

impl WorkerAgentGrpc {
    /// The stored TOML of one revision, if it is still retained.
    async fn revision_toml(
        &self,
        server: &crate::entities::surreal::server::ServerId,
        revision: i64,
    ) -> Option<String> {
        self.db
            .process(FindServerConfigRevision {
                server: server.clone(),
                revision,
            })
            .await
            .ok()
            .flatten()
            .map(|row| row.toml)
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
        let server = self
            .db
            .process(FindServerById {
                id: agent.server.clone(),
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Not found"))?;

        let server_key = ids::record_key(&server.id.0);
        let subscription =
            self.hub
                .subscribe(&server_key, agent.generation, server.applied_revision);
        let (tx, rx) = mpsc::channel(STREAM_CAPACITY);

        // Send what the server should be running right now; revision 0 means the
        // canvas has never been stamped and there is nothing to send yet.
        if server.desired_revision > 0
            && let Some(toml) = self
                .revision_toml(&server.id, server.desired_revision)
                .await
            && tx
                .send(Ok(pb::ConfigRevision {
                    revision: server.desired_revision,
                    toml,
                }))
                .await
                .is_err()
        {
            return Ok(Response::new(ReceiverStream::new(rx)));
        }

        let this = self.clone();
        let server_id = server.id.clone();
        tokio::spawn(async move {
            let mut subscription = subscription;
            loop {
                match subscription.rx.recv().await {
                    Ok(AgentSignal::Revision(revision)) => {
                        let Some(toml) = this.revision_toml(&server_id, revision).await else {
                            continue; // pruned already; a newer signal will follow
                        };
                        if tx
                            .send(Ok(pb::ConfigRevision { revision, toml }))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Ok(AgentSignal::Superseded) => {
                        let _ = tx
                            .send(Err(Status::unauthenticated("refresh key superseded")))
                            .await;
                        return;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        // Re-read the row: only the latest revision matters.
                        let Ok(Some(row)) = this
                            .db
                            .process(FindServerById {
                                id: server_id.clone(),
                            })
                            .await
                        else {
                            return;
                        };
                        let Some(toml) = this.revision_toml(&server_id, row.desired_revision).await
                        else {
                            continue;
                        };
                        if tx
                            .send(Ok(pb::ConfigRevision {
                                revision: row.desired_revision,
                                toml,
                            }))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
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
