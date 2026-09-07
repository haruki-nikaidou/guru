use crate::entities::surreal::node::NodeId;
use crate::entities::surreal::server::ServerId;
use chrono::{DateTime, Utc};
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(ServerHealthRecordId, "server_health_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerHealthRecordEntity {
    pub id: ServerHealthRecordId,
    pub server: ServerId,
    pub status: ServerHealthStatus,
    pub report_time: DateTime<Utc>,
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub max_connection_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
pub enum ServerHealthStatus {
    /// The server is online and healthy.
    Online,
    /// The server failed to apply the latest configuration changes.
    Downgraded,
    /// The server is offline or can't connect to the master node.
    Offline,
}

pub struct ListServerHealthHistory {
    pub server: ServerId,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl Processor<ListServerHealthHistory> for SurrealProcessor {
    type Output = Vec<ServerHealthRecordEntity>;
    type Error = surrealdb::Error;
    async fn process(&self, input: ListServerHealthHistory) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

table_record!(NodeHealthyRecordId, "node_health_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeHealthRecordEntity {
    pub id: NodeHealthyRecordId,
    pub node: NodeId,
    pub status: NodeHealthStatus,
    pub message: String,
    pub report_time: DateTime<Utc>,
}

#[derive(Debug, Clone, SurrealValue)]
pub enum NodeHealthStatus {
    /// The node setting is applied to the server, and the node is healthy
    Ready,

    /// Waiting server to sync the new config
    Deploying,

    /// The node failed to apply the latest configuration changes.
    Failed,
}

pub struct ListNodeHealthHistory {
    pub node: NodeId,
    pub limit: u64,
    pub offset: u64,
}

impl Processor<ListNodeHealthHistory> for SurrealProcessor {
    type Output = Vec<NodeHealthRecordEntity>;
    type Error = surrealdb::Error;
    async fn process(&self, input: ListNodeHealthHistory) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
