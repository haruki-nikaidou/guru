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
}

#[derive(Debug)]
pub struct ConnectPorts {
    pub source: PortId,
    pub target: PortId,
    pub canvas: CanvasId,
}

impl Processor<ConnectPorts> for SurrealProcessor {
    type Output = EdgeConnectionEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:ConnectPorts", skip_all, err)]
    async fn process(&self, input: ConnectPorts) -> Result<Self::Output, Self::Error> {
        // Statement 0 is BEGIN; the RETURN below is statement 3.
        let mut resp = self
            .db()
            .query(
                "BEGIN TRANSACTION;
                 LET $edge = (RELATE ONLY $source->orchestration_edge_connection->$target);
                 fn::orchestration_touch($canvas);
                 RETURN $edge;
                 COMMIT TRANSACTION;",
            )
            .bind(("source", input.source))
            .bind(("target", input.target))
            .bind(("canvas", input.canvas))
            .await?;
        resp.take::<Option<EdgeConnectionEntity>>(3)?
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
    #[tracing::instrument(name = "Query:FindEdgeById", skip_all, err)]
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
pub struct DeleteEdgeRow {
    pub id: EdgeConnectionId,
    pub canvas: CanvasId,
}

impl Processor<DeleteEdgeRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:DeleteEdgeRow", skip_all, err)]
    async fn process(&self, input: DeleteEdgeRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "BEGIN TRANSACTION;
                 DELETE $id;
                 fn::orchestration_touch($canvas);
                 COMMIT TRANSACTION;",
            )
            .bind(("id", input.id))
            .bind(("canvas", input.canvas))
            .await?
            .check()?;
        Ok(())
    }
}
