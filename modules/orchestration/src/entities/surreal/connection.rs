use surrealdb::types::SurrealValue;
use newtype_record_id::table_record;
use crate::entities::surreal::port::PortId;

table_record!(EdgeConnectionId, "edge_connection");

#[derive(Debug, Clone, SurrealValue)]
pub struct EdgeConnectionEntity {
    pub id: EdgeConnectionId,
    #[surreal(rename = "in")]
    pub source: PortId,
    #[surreal(rename = "out")]
    pub target: PortId,
}