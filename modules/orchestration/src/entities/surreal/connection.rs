use crate::entities::surreal::canvas::CanvasId;
use crate::entities::surreal::port::PortId;
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(EdgeConnectionId, "orchestration_edge_connection");

#[derive(Debug, Clone, SurrealValue)]
pub struct EdgeConnectionEntity {
    pub id: EdgeConnectionId,
    #[surreal(rename = "in")]
    pub source: PortId,
    #[surreal(rename = "out")]
    pub target: PortId,
    pub created_rev: i64,
    pub retired_rev: Option<i64>,
}

#[derive(Debug)]
pub struct ConnectPorts {
    pub source: PortId,
    pub target: PortId,
    pub revision: i64,
}

impl Processor<ConnectPorts> for SurrealProcessor {
    type Output = EdgeConnectionEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ConnectPorts", skip(self), err)]
    async fn process(&self, input: ConnectPorts) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "RELATE ONLY $source->orchestration_edge_connection->$target
                 CONTENT { created_rev: $revision, retired_rev: NONE }",
            )
            .bind(("source", input.source))
            .bind(("target", input.target))
            .bind(("revision", input.revision))
            .await?;
        resp.take::<Option<EdgeConnectionEntity>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("relate returned no row".to_string()))
    }
}

#[derive(Debug)]
pub struct FindEdgeById {
    pub id: EdgeConnectionId,
}

impl Processor<FindEdgeById> for SurrealProcessor {
    type Output = Option<EdgeConnectionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindEdgeById", skip(self), err)]
    async fn process(&self, input: FindEdgeById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<EdgeConnectionEntity>>(0)
    }
}

#[derive(Debug)]
pub struct ListLiveEdgesByCanvas {
    pub canvas: CanvasId,
}

impl Processor<ListLiveEdgesByCanvas> for SurrealProcessor {
    type Output = Vec<EdgeConnectionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(
        name = "Query:ListLiveEdgesByCanvas",
        skip(self),
        err,
        fields(result_count)
    )]
    async fn process(&self, input: ListLiveEdgesByCanvas) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT * FROM orchestration_edge_connection
                 WHERE in.owner.canvas = $canvas AND retired_rev IS NONE",
            )
            .bind(("canvas", input.canvas))
            .await?;
        let result = resp.take::<Vec<EdgeConnectionEntity>>(0)?;
        tracing::Span::current().record("result_count", result.len());
        Ok(result)
    }
}

#[derive(Debug)]
/// Live **and** retiring edges, for the dashboard view of a rollout in flight.
pub struct ListEdgesByCanvas {
    pub canvas: CanvasId,
}

impl Processor<ListEdgesByCanvas> for SurrealProcessor {
    type Output = Vec<EdgeConnectionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(
        name = "Query:ListEdgesByCanvas",
        skip(self),
        err,
        fields(result_count)
    )]
    async fn process(&self, input: ListEdgesByCanvas) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM orchestration_edge_connection WHERE in.owner.canvas = $canvas")
            .bind(("canvas", input.canvas))
            .await?;
        let result = resp.take::<Vec<EdgeConnectionEntity>>(0)?;
        tracing::Span::current().record("result_count", result.len());
        Ok(result)
    }
}

#[derive(Debug)]
pub struct FindLiveEdgeByPort {
    pub port: PortId,
}

impl Processor<FindLiveEdgeByPort> for SurrealProcessor {
    type Output = Option<EdgeConnectionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindLiveEdgeByPort", skip(self), err)]
    async fn process(&self, input: FindLiveEdgeByPort) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT * FROM orchestration_edge_connection
                 WHERE retired_rev IS NONE AND (in = $port OR out = $port) LIMIT 1",
            )
            .bind(("port", input.port))
            .await?;
        resp.take::<Option<EdgeConnectionEntity>>(0)
    }
}

#[derive(Debug)]
pub struct RetireEdgeRow {
    pub id: EdgeConnectionId,
    pub revision: i64,
}

impl Processor<RetireEdgeRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:RetireEdgeRow", skip(self), err)]
    async fn process(&self, input: RetireEdgeRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "UPDATE orchestration_edge_connection SET retired_rev = $revision
                 WHERE id = $id AND retired_rev IS NONE",
            )
            .bind(("id", input.id))
            .bind(("revision", input.revision))
            .await?
            .check()?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ForceDeleteEdgeRow {
    pub id: EdgeConnectionId,
}

impl Processor<ForceDeleteEdgeRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ForceDeleteEdgeRow", skip(self), err)]
    async fn process(&self, input: ForceDeleteEdgeRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE orchestration_edge_connection WHERE id = $id")
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}
