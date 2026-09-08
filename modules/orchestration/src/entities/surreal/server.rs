use crate::entities::surreal::canvas::{CanvasId, CanvasUiPosition};
use chrono::{DateTime, Utc};
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(ServerId, "orchestration_server");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerEntity {
    pub id: ServerId,
    pub canvas: CanvasId,
    pub name: String,
    pub icon: String,
    pub comment: String,
    pub position: CanvasUiPosition,
    pub ipv6_resolve: ServerIpv6Resolve,
    pub log_level: String,
    pub desired_revision: i64,
    pub applied_revision: i64,
    pub last_apply_error: Option<String>,
    pub current_dynamic_refresh_key: Option<String>,
    pub refresh_key_generation: i64,
    pub last_seen_at: Option<DateTime<Utc>>,
}

/// Local mirror of [`guru_worker_config::Ipv6Resolve`]; `SurrealValue` cannot be
/// derived for a foreign type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum ServerIpv6Resolve {
    Required,
    Preferred,
    Tolerated,
    Forbidden,
}

impl From<ServerIpv6Resolve> for guru_worker_config::Ipv6Resolve {
    fn from(value: ServerIpv6Resolve) -> Self {
        match value {
            ServerIpv6Resolve::Required => guru_worker_config::Ipv6Resolve::Required,
            ServerIpv6Resolve::Preferred => guru_worker_config::Ipv6Resolve::Preferred,
            ServerIpv6Resolve::Tolerated => guru_worker_config::Ipv6Resolve::Tolerated,
            ServerIpv6Resolve::Forbidden => guru_worker_config::Ipv6Resolve::Forbidden,
        }
    }
}

table_record!(ServerIpRecordId, "server_ip_record");

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerIpRecordEntity {
    pub id: ServerIpRecordId,
    pub server: ServerId,
    pub ip: String,
    pub country: String,
}

#[derive(Debug, Clone)]
pub struct ServerWithIp {
    pub server: ServerEntity,
    pub ips: Vec<ServerIpRecordEntity>,
}

pub struct CreateServer {
    pub canvas: CanvasId,
    pub name: String,
    pub icon: String,
    pub comment: String,
    pub position: CanvasUiPosition,
    pub ipv6_resolve: ServerIpv6Resolve,
    pub log_level: String,
}

impl Processor<CreateServer> for SurrealProcessor {
    type Output = ServerEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:CreateServer", skip_all, err)]
    async fn process(&self, input: CreateServer) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE ONLY orchestration_server CONTENT {
                     canvas: $canvas, name: $name, icon: $icon, comment: $comment,
                     position: $position, ipv6_resolve: $ipv6_resolve, log_level: $log_level,
                     desired_revision: 0, applied_revision: 0, last_apply_error: NONE,
                     current_dynamic_refresh_key: NONE, refresh_key_generation: 0,
                     last_seen_at: NONE
                 }",
            )
            .bind(("canvas", input.canvas))
            .bind(("name", input.name))
            .bind(("icon", input.icon))
            .bind(("comment", input.comment))
            .bind(("position", input.position))
            .bind(("ipv6_resolve", input.ipv6_resolve))
            .bind(("log_level", input.log_level))
            .await?;
        resp.take::<Option<ServerEntity>>(0)?.ok_or_else(|| {
            surrealdb::Error::internal("create server returned no row".to_string())
        })
    }
}

pub struct FindServerById {
    pub id: ServerId,
}

impl Processor<FindServerById> for SurrealProcessor {
    type Output = Option<ServerEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindServerById", skip_all, err)]
    async fn process(&self, input: FindServerById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<ServerEntity>>(0)
    }
}

pub struct ListServersByCanvas {
    pub canvas: CanvasId,
}

impl Processor<ListServersByCanvas> for SurrealProcessor {
    type Output = Vec<ServerEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListServersByCanvas", skip_all, err)]
    async fn process(&self, input: ListServersByCanvas) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM orchestration_server WHERE canvas = $canvas")
            .bind(("canvas", input.canvas))
            .await?;
        resp.take::<Vec<ServerEntity>>(0)
    }
}

pub struct UpdateServerSettings {
    pub id: ServerId,
    pub name: String,
    pub icon: String,
    pub comment: String,
    pub ipv6_resolve: ServerIpv6Resolve,
    pub log_level: String,
}

impl Processor<UpdateServerSettings> for SurrealProcessor {
    type Output = ServerEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateServerSettings", skip_all, err)]
    async fn process(&self, input: UpdateServerSettings) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "UPDATE $id SET name = $name, icon = $icon, comment = $comment,
                     ipv6_resolve = $ipv6_resolve, log_level = $log_level RETURN AFTER",
            )
            .bind(("id", input.id))
            .bind(("name", input.name))
            .bind(("icon", input.icon))
            .bind(("comment", input.comment))
            .bind(("ipv6_resolve", input.ipv6_resolve))
            .bind(("log_level", input.log_level))
            .await?;
        resp.take::<Option<ServerEntity>>(0)?
            .ok_or_else(|| surrealdb::Error::internal("server not found".to_string()))
    }
}

pub struct MoveServerPosition {
    pub id: ServerId,
    pub position: CanvasUiPosition,
}

impl Processor<MoveServerPosition> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:MoveServerPosition", skip_all, err)]
    async fn process(&self, input: MoveServerPosition) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $id SET position = $position")
            .bind(("id", input.id))
            .bind(("position", input.position))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct DeleteServerRow {
    pub id: ServerId,
}

impl Processor<DeleteServerRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query-Transaction:DeleteServerRow", skip_all, err)]
    async fn process(&self, input: DeleteServerRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query(
                "BEGIN TRANSACTION;
                 DELETE orchestration_server_config_revision WHERE server = $id;
                 DELETE server_ip_record WHERE server = $id;
                 DELETE $id;
                 COMMIT TRANSACTION;",
            )
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct CreateServerIp {
    pub server: ServerId,
    pub ip: String,
    pub country: String,
}

impl Processor<CreateServerIp> for SurrealProcessor {
    type Output = ServerIpRecordEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:CreateServerIp", skip_all, err)]
    async fn process(&self, input: CreateServerIp) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE ONLY server_ip_record CONTENT { server: $server, ip: $ip, country: $country }",
            )
            .bind(("server", input.server))
            .bind(("ip", input.ip))
            .bind(("country", input.country))
            .await?;
        resp.take::<Option<ServerIpRecordEntity>>(0)?.ok_or_else(|| {
            surrealdb::Error::internal("create server ip returned no row".to_string())
        })
    }
}

pub struct FindServerIpById {
    pub id: ServerIpRecordId,
}

impl Processor<FindServerIpById> for SurrealProcessor {
    type Output = Option<ServerIpRecordEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindServerIpById", skip_all, err)]
    async fn process(&self, input: FindServerIpById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<ServerIpRecordEntity>>(0)
    }
}

pub struct ListServerIpsByCanvas {
    pub canvas: CanvasId,
}

impl Processor<ListServerIpsByCanvas> for SurrealProcessor {
    type Output = Vec<ServerIpRecordEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListServerIpsByCanvas", skip_all, err)]
    async fn process(&self, input: ListServerIpsByCanvas) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM server_ip_record WHERE server.canvas = $canvas")
            .bind(("canvas", input.canvas))
            .await?;
        resp.take::<Vec<ServerIpRecordEntity>>(0)
    }
}

pub struct DeleteServerIpRow {
    pub id: ServerIpRecordId,
}

impl Processor<DeleteServerIpRow> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:DeleteServerIpRow", skip_all, err)]
    async fn process(&self, input: DeleteServerIpRow) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE $id")
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}

/// Replaces the stored refresh-key digest and returns the new generation, which
/// invalidates every stream still holding the previous key.
pub struct RotateServerRefreshKey {
    pub server: ServerId,
    pub digest: String,
    pub now: DateTime<Utc>,
}

impl Processor<RotateServerRefreshKey> for SurrealProcessor {
    type Output = i64;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:RotateServerRefreshKey", skip_all, err)]
    async fn process(&self, input: RotateServerRefreshKey) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "UPDATE ONLY $server SET current_dynamic_refresh_key = $digest,
                     refresh_key_generation += 1, last_seen_at = $now RETURN AFTER",
            )
            .bind(("server", input.server))
            .bind(("digest", input.digest))
            .bind(("now", input.now))
            .await?;
        resp.take::<Option<ServerEntity>>(0)?
            .map(|s| s.refresh_key_generation)
            .ok_or_else(|| surrealdb::Error::internal("server not found".to_string()))
    }
}

pub struct FindServerByRefreshKeyDigest {
    pub digest: String,
}

impl Processor<FindServerByRefreshKeyDigest> for SurrealProcessor {
    type Output = Option<ServerEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindServerByRefreshKeyDigest", skip_all, err)]
    async fn process(
        &self,
        input: FindServerByRefreshKeyDigest,
    ) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT * FROM orchestration_server WHERE current_dynamic_refresh_key = $digest LIMIT 1",
            )
            .bind(("digest", input.digest))
            .await?;
        resp.take::<Option<ServerEntity>>(0)
    }
}

pub struct SetServerDesiredRevision {
    pub server: ServerId,
    pub revision: i64,
}

impl Processor<SetServerDesiredRevision> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:SetServerDesiredRevision", skip_all, err)]
    async fn process(&self, input: SetServerDesiredRevision) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $server SET desired_revision = $revision, last_apply_error = NONE")
            .bind(("server", input.server))
            .bind(("revision", input.revision))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct MarkServerApplied {
    pub server: ServerId,
    pub revision: i64,
}

impl Processor<MarkServerApplied> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:MarkServerApplied", skip_all, err)]
    async fn process(&self, input: MarkServerApplied) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $server SET applied_revision = $revision, last_apply_error = NONE")
            .bind(("server", input.server))
            .bind(("revision", input.revision))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct SetServerApplyError {
    pub server: ServerId,
    pub error: String,
}

impl Processor<SetServerApplyError> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:SetServerApplyError", skip_all, err)]
    async fn process(&self, input: SetServerApplyError) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $server SET last_apply_error = $error")
            .bind(("server", input.server))
            .bind(("error", input.error))
            .await?
            .check()?;
        Ok(())
    }
}

#[derive(Debug, Clone, SurrealValue)]
pub struct ServerWatchState {
    pub id: ServerId,
    pub desired_revision: i64,
    pub refresh_key_generation: i64,
}

pub struct ListServerWatchState {
    pub servers: Vec<ServerId>,
}

impl Processor<ListServerWatchState> for SurrealProcessor {
    type Output = Vec<ServerWatchState>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListServerWatchState", skip_all, err)]
    async fn process(&self, input: ListServerWatchState) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "SELECT id, desired_revision, refresh_key_generation
                 FROM orchestration_server WHERE id IN $servers",
            )
            .bind(("servers", input.servers))
            .await?;
        resp.take::<Vec<ServerWatchState>>(0)
    }
}
