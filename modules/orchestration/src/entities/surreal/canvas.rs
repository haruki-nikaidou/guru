use crate::entities::surreal::connection::EdgeConnectionEntity;
use crate::entities::surreal::node::NodeWithPorts;
use crate::entities::surreal::server::ServerWithIp;
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(CanvasId, "orchestration_canvas");

#[derive(Debug, Clone, SurrealValue)]
pub struct CanvasEntity {
    pub id: CanvasId,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
pub struct CanvasUiPosition {
    pub x: i64,
    pub y: i64,
}

/// Everything the dashboard renders for one canvas: live rows **and** rows that are
/// retired but not yet garbage-collected, so a rollout in flight stays visible.
#[derive(Debug, Clone)]
pub struct CanvasContents {
    pub canvas: CanvasEntity,
    pub servers: Vec<ServerWithIp>,
    pub nodes: Vec<NodeWithPorts>,
    pub edges: Vec<EdgeConnectionEntity>,
}

#[derive(Debug)]
pub struct CreateCanvas {
    pub name: String,
    pub description: String,
}

impl Processor<CreateCanvas> for SurrealProcessor {
    type Output = CanvasEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:CreateCanvas", skip(self), err)]
    async fn process(&self, input: CreateCanvas) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE ONLY orchestration_canvas CONTENT { name: $name, description: $description }",
            )
            .bind(("name", input.name))
            .bind(("description", input.description))
            .await?;
        resp.take::<Option<CanvasEntity>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("create canvas returned no row".to_string()))
    }
}

#[derive(Debug)]
pub struct ListCanvases;

impl Processor<ListCanvases> for SurrealProcessor {
    type Output = Vec<CanvasEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListCanvases", skip(self), err, fields(result_count))]
    async fn process(&self, _input: ListCanvases) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM orchestration_canvas")
            .await?;
        let result = resp.take::<Vec<CanvasEntity>>(0)?;
        tracing::Span::current().record("result_count", result.len());
        Ok(result)
    }
}

#[derive(Debug)]
pub struct FindCanvasById {
    pub id: CanvasId,
}

impl Processor<FindCanvasById> for SurrealProcessor {
    type Output = Option<CanvasEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindCanvasById", skip(self), err)]
    async fn process(&self, input: FindCanvasById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<CanvasEntity>>(0)
    }
}

#[derive(Debug)]
pub struct UpdateCanvasMeta {
    pub id: CanvasId,
    pub name: String,
    pub description: String,
}

impl Processor<UpdateCanvasMeta> for SurrealProcessor {
    type Output = CanvasEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateCanvasMeta", skip_all, err, fields(canvas_id = ?input.id))]
    async fn process(&self, input: UpdateCanvasMeta) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("UPDATE $id SET name = $name, description = $description RETURN AFTER")
            .bind(("id", input.id))
            .bind(("name", input.name))
            .bind(("description", input.description))
            .await?;
        resp.take::<Option<CanvasEntity>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("canvas not found".to_string()))
    }
}

#[derive(Debug)]
pub struct DeleteCanvasRow {
    pub id: CanvasId,
}

impl Processor<DeleteCanvasRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:DeleteCanvasRow", skip_all, err, fields(canvas_id = ?input.id))]
    async fn process(&self, input: DeleteCanvasRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "BEGIN TRANSACTION;
                 LET $servers = (SELECT VALUE id FROM orchestration_server WHERE canvas = $id);
                 DELETE orchestration_server_config_revision WHERE server IN $servers;
                 DELETE server_ip_record WHERE server IN $servers;
                 DELETE orchestration_server WHERE id IN $servers;
                 DELETE $id;
                 COMMIT TRANSACTION;",
            )
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}
