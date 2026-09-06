use crate::entities::surreal::node::{NodeEntity, NodeWithPorts};
use crate::entities::surreal::port::PortEntity;
use crate::entities::surreal::server::ServerWithIp;
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use wakuwaku::surreal::SurrealProcessor;
use crate::entities::surreal::connection::EdgeConnectionEntity;

table_record!(CanvasId, "orchestration_canvas");

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasEntity {
    pub id: CanvasId,
    pub parent_canvas: Option<CanvasId>,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasUiPosition {
    pub x: i64,
    pub y: i64,
}

pub struct ListEverythingInCanva {
    pub id: Uuid,
}

pub struct CanvaContents {
    pub sub_canvas: Vec<CanvasEntity>,
    pub servers: Vec<ServerWithIp>,
    pub nodes: Vec<NodeWithPorts>,
    pub connections: Vec<EdgeConnectionEntity>
}

impl Processor<ListEverythingInCanva> for SurrealProcessor {
    type Output = CanvaContents;
    type Error = surrealdb::Error;
    async fn process(&self, input: ListEverythingInCanva) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
