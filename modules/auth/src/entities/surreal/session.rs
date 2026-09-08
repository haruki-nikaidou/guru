use super::account::AccountId;
use chrono::{DateTime, Utc};
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(SessionId, "auth_session");

#[derive(Debug, Clone, SurrealValue)]
pub struct SessionEntity {
    pub id: SessionId,
    pub account_id: AccountId,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

pub struct FindSessionById {
    pub session_id: String,
}

impl Processor<FindSessionById> for SurrealProcessor {
    type Output = Option<SessionEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindSessionById", skip_all, err)]
    async fn process(&self, input: FindSessionById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM type::record('auth_session', $id)")
            .bind(("id", input.session_id))
            .await?;
        resp.take::<Option<SessionEntity>>(0)
    }
}

/// Create a session whose record key is the opaque session token itself.
pub struct CreateSession {
    pub token: String,
    pub account_id: AccountId,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}

impl Processor<CreateSession> for SurrealProcessor {
    type Output = SessionEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:CreateSession", skip_all, err)]
    async fn process(&self, input: CreateSession) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE type::record('auth_session', $session_key) CONTENT { account_id: $account_id, user_agent: $user_agent, created_at: $created_at, last_active_at: $last_active_at }",
            )
            .bind(("session_key", input.token))
            .bind(("account_id", input.account_id))
            .bind(("user_agent", input.user_agent))
            .bind(("created_at", input.created_at))
            .bind(("last_active_at", input.last_active_at))
            .await?;
        resp.take::<Option<SessionEntity>>(0)?.ok_or_else(|| {
            surrealdb::Error::internal("create auth_session returned no row".to_string())
        })
    }
}

pub struct DeleteSession {
    pub session_id: String,
}

impl Processor<DeleteSession> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:DeleteSession", skip_all, err)]
    async fn process(&self, input: DeleteSession) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE type::record('auth_session', $id)")
            .bind(("id", input.session_id))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct UpdateSession {
    pub id: String,
    pub last_active_at: DateTime<Utc>,
}

impl Processor<UpdateSession> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateSession", skip_all, err)]
    async fn process(&self, input: UpdateSession) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE type::record('auth_session', $id) SET last_active_at = $last_active_at")
            .bind(("id", input.id))
            .bind(("last_active_at", input.last_active_at))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct DeleteSessionsByAccount {
    pub account_id: AccountId,
}

impl Processor<DeleteSessionsByAccount> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:DeleteSessionsByAccount", skip_all, err)]
    async fn process(&self, input: DeleteSessionsByAccount) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE auth_session WHERE account_id = $account_id")
            .bind(("account_id", input.account_id))
            .await?
            .check()?;
        Ok(())
    }
}
