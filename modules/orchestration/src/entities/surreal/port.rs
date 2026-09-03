use crate::entities::surreal::node::NodeId;
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(PortId, "orchestration_port");

#[derive(Debug, Clone, SurrealValue)]
pub struct PortEntity {
    pub id: PortId,
    pub owner: NodeId,
    pub kind: PortKind,
    pub direction: PortDirection,
    pub key: String,
    pub position: i64,
}

#[derive(Debug, Clone, SurrealValue, Copy, PartialEq, Eq)]
#[surreal(untagged)]
pub enum PortKind {
    #[surreal(value = "derive_listen")]
    DeriveListen,
    #[surreal(value = "derive_destination")]
    DeriveDestination,
}

#[derive(Debug, Clone, SurrealValue, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
}
