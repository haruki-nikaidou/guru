use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(ServerId, "orchestration_server");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerEntity {
    pub id: ServerId,
    pub canvas: CanvasId,
    pub name: String,
    pub icon: String,
    pub comment: String,
    pub position: CanvasUiPosition,
    pub current_dynamic_refresh_key: Option<String>,
}

table_record!(ServerIpRecordId, "server_ip_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerIpRecordEntity {
    pub id: ServerIpRecordId,
    pub server: ServerId,
    pub ip: String,
    pub current_unique_key: Option<String>,
    pub country: String,
}
