use crate::entities::surreal::port::PortId;
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(EdgeConnectionId, "orchestration_edge_connection");

#[derive(Debug, Clone, SurrealValue)]
pub struct EdgeConnectionEntity {
    pub id: EdgeConnectionId,
    #[surreal(rename = "in")]
    pub source: PortId,
    #[surreal(rename = "out")]
    pub target: PortId,
}
