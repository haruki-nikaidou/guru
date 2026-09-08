//! Health records reported by workers.
//!
//! Stage 1 only declares the row types; the queries land with the health push
//! pipeline in a later stage.

use crate::entities::surreal::node::NodeId;
use crate::entities::surreal::server::ServerId;
use chrono::{DateTime, Utc};
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

table_record!(ServerHealthRecordId, "server_health_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerHealthRecordEntity {
    pub id: ServerHealthRecordId,
    pub server: ServerId,
    pub status: ServerHealthStatus,
    pub report_time: DateTime<Utc>,
    pub upload_bytes: i64,
    pub download_bytes: i64,
    pub max_connection_number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum ServerHealthStatus {
    /// The server is online and healthy.
    Online,
    /// The server failed to apply the latest configuration changes.
    Degraded,
    /// The server is offline or can't connect to the master node.
    Offline,
}

pub struct ListServerHealthHistory {
    pub server: ServerId,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

table_record!(NodeHealthRecordId, "node_health_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeHealthRecordEntity {
    pub id: NodeHealthRecordId,
    pub node: NodeId,
    pub status: NodeHealthStatus,
    pub message: String,
    pub report_time: DateTime<Utc>,
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
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
    pub limit: i64,
    pub offset: i64,
}
