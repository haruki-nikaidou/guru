use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(CanvasId, "canvas");

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasEntity {
    pub id: CanvasId,
    pub name: String,
    pub description: String,
}
