//! Per-server config revisions and the RCU garbage collector.
//!
//! A revision row retains the exact node and edge rows a server's running config was
//! derived from. A retired node or edge may only be deleted once no retained row
//! references it any more.

use crate::entities::surreal::connection::EdgeConnectionId;
use crate::entities::surreal::node::NodeId;
use crate::entities::surreal::server::ServerId;
use chrono::{DateTime, Utc};
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(
    ServerConfigRevisionId,
    "orchestration_server_config_revision"
);

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerConfigRevisionEntity {
    pub id: ServerConfigRevisionId,
    pub server: ServerId,
    pub revision: i64,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeConnectionId>,
    pub toml: String,
    pub created_at: DateTime<Utc>,
}

/// Allocates the next global revision.
///
/// `sequence::nextval` yields `0` on its first call, while `0` is the reserved
/// sentinel for "never stamped" (a fresh server) and "no last-known-good file"
/// (a fresh worker), so every real revision starts at `1`.
pub struct NextRevision;

impl Processor<NextRevision> for SurrealProcessor {
    type Output = i64;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:NextRevision", skip_all, err)]
    async fn process(&self, _input: NextRevision) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("RETURN sequence::nextval('orchestration_revision') + 1")
            .await?;
        resp.take::<Option<i64>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("sequence returned no value".to_string()))
    }
}

pub struct RecordServerConfigRevision {
    pub server: ServerId,
    pub revision: i64,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeConnectionId>,
    pub toml: String,
}

impl Processor<RecordServerConfigRevision> for SurrealProcessor {
    type Output = ServerConfigRevisionEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:RecordServerConfigRevision", skip_all, err, fields(server = ?input.server))]
    async fn process(
        &self,
        input: RecordServerConfigRevision,
    ) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE ONLY orchestration_server_config_revision CONTENT {
                     server: $server, revision: $revision, nodes: $nodes,
                     edges: $edges, toml: $toml, created_at: time::now()
                 }",
            )
            .bind(("server", input.server))
            .bind(("revision", input.revision))
            .bind(("nodes", input.nodes))
            .bind(("edges", input.edges))
            .bind(("toml", input.toml))
            .await?;
        resp.take::<Option<ServerConfigRevisionEntity>>(0)?
            .ok_or_else(|| {
                surrealdb::Error::internal("create config revision returned no row".to_string())
            })
    }
}

#[derive(Debug)]
pub struct FindServerConfigRevision {
    pub server: ServerId,
    pub revision: i64,
}

impl Processor<FindServerConfigRevision> for SurrealProcessor {
    type Output = Option<ServerConfigRevisionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindServerConfigRevision", skip(self), err)]
    async fn process(&self, input: FindServerConfigRevision) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT * FROM orchestration_server_config_revision
                 WHERE server = $server AND revision = $revision LIMIT 1",
            )
            .bind(("server", input.server))
            .bind(("revision", input.revision))
            .await?;
        resp.take::<Option<ServerConfigRevisionEntity>>(0)
    }
}

#[derive(Debug)]
pub struct ListRetainedRevisions {
    pub server: ServerId,
}

impl Processor<ListRetainedRevisions> for SurrealProcessor {
    type Output = Vec<ServerConfigRevisionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListRetainedRevisions", skip(self), err)]
    async fn process(&self, input: ListRetainedRevisions) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT * FROM orchestration_server_config_revision
                 WHERE server = $server ORDER BY revision",
            )
            .bind(("server", input.server))
            .await?;
        resp.take::<Vec<ServerConfigRevisionEntity>>(0)
    }
}

#[derive(Debug)]
pub struct PruneServerRevisionsBelow {
    pub server: ServerId,
    pub revision: i64,
}

impl Processor<PruneServerRevisionsBelow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:PruneServerRevisionsBelow", skip(self), err)]
    async fn process(&self, input: PruneServerRevisionsBelow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "DELETE orchestration_server_config_revision
                 WHERE server = $server AND revision < $revision",
            )
            .bind(("server", input.server))
            .bind(("revision", input.revision))
            .await?
            .check()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
pub struct GcReport {
    pub nodes: i64,
    pub edges: i64,
}

pub struct CollectRcuGarbage;

impl Processor<CollectRcuGarbage> for SurrealProcessor {
    type Output = GcReport;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:CollectRcuGarbage", skip_all, err)]
    async fn process(&self, _input: CollectRcuGarbage) -> Result<Self::Output, Self::Error> {
        // Statement 0 is BEGIN; the RETURN below is statement 9.
        let mut resp = self
            .db()
            .query(
                "BEGIN TRANSACTION;
                 LET $nodes_ref = array::flatten((SELECT VALUE nodes FROM orchestration_server_config_revision));
                 LET $edges_ref = array::flatten((SELECT VALUE edges FROM orchestration_server_config_revision));
                 LET $dead_nodes = (SELECT VALUE id FROM orchestration_node
                     WHERE retired_rev IS NOT NONE AND id NOT IN $nodes_ref);
                 LET $dead_ports = (SELECT VALUE id FROM orchestration_port WHERE owner IN $dead_nodes);
                 LET $dead_edges = (SELECT VALUE id FROM orchestration_edge_connection
                     WHERE (retired_rev IS NOT NONE AND id NOT IN $edges_ref)
                        OR in IN $dead_ports OR out IN $dead_ports);
                 DELETE orchestration_edge_connection WHERE id IN $dead_edges;
                 DELETE orchestration_port WHERE id IN $dead_ports;
                 DELETE orchestration_node WHERE id IN $dead_nodes;
                 RETURN { nodes: array::len($dead_nodes), edges: array::len($dead_edges) };
                 COMMIT TRANSACTION;",
            )
            .await?;
        resp.take::<Option<GcReport>>(9)?
            .ok_or_else(|| surrealdb::Error::internal("gc returned no report".to_string()))
    }
}
