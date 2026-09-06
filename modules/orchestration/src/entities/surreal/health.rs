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
    /// The server failed to apply latest configuration changes.
    Downgraded,
    /// The server is offline or can't connect to master node.
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
