use newtype_record_id::table_record;
use chrono::{DateTime, Utc};
use super::account::AccountId;
use surrealdb_types::SurrealValue;

table_record!(SessionId, "auth_session");

#[derive(Debug, Clone, SurrealValue)]
pub struct SessionEntity {
    pub id: SessionId,
    pub account_id: AccountId,
    pub user_agent: String,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
}
