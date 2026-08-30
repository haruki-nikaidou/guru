use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(ServerId, "server");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerEntity {
    pub id: ServerId,
    pub canvas: CanvasId,
    pub name: String,
    pub icon: String,
    pub comment: String,
    pub position: CanvasUiPosition,
}

table_record!(ServerIpRecordId, "ip_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerIpRecordEntity {
    pub id: ServerIpRecordId,
    pub server: ServerId,
    pub ip: String,
    pub country: String,
}
