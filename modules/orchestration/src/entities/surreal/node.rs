use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use crate::entities::surreal::server::ServerId;
use newtype_record_id::table_record;
use surrealdb::types::SurrealValue;

table_record!(NodeId, "node");

#[derive(Debug, Clone, SurrealValue)]
pub struct NodeEntity {
    pub id: NodeId,
    pub canvas: CanvasId,
    pub name: String,
    pub comment: String,
    pub spec: NodeSpec,
    pub server: Option<ServerId>,
    pub position: CanvasUiPosition,
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(tag = "type", content = "config", rename_all = "snake_case")]
pub enum NodeSpec {
    Pod(PodConfig),
    Entry(EntryConfig),
    Relay(RelayConfig),
    Exit(ExitConfig),
    LoadBalanceDistribute(LoadBalanceDistributeConfig),
    LoadBalanceAggregate(LoadBalanceAggregateConfig),
}

#[derive(Debug, Clone, SurrealValue)]
pub struct PodConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct EntryConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct RelayConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct ExitConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct LoadBalanceDistributeConfig {}

#[derive(Debug, Clone, SurrealValue)]
pub struct LoadBalanceAggregateConfig {}
