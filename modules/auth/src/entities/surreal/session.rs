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
    async fn process(&self, input: FindSessionById) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct CreateSession {
    pub account_id: AccountId,
    pub user_agent: String,
}

impl Processor<CreateSession> for SurrealProcessor {
    type Output = SessionEntity;
    type Error = surrealdb::Error;
    async fn process(&self, input: CreateSession) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct DeleteSession {
    pub id: SessionId,
}

impl Processor<DeleteSession> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    async fn process(&self, input: DeleteSession) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct UpdateSession {
    pub id: String,
    pub last_active_at: DateTime<Utc>,
}

impl Processor<UpdateSession> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    async fn process(&self, input: UpdateSession) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
