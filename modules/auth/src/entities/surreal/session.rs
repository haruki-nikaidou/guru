use super::account::AccountId;
use chrono::{DateTime, NaiveDateTime, Utc};
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

table_record!(SessionId, "auth_session");

#[derive(Debug, Clone, SurrealValue)]
pub struct SessionEntity {
    pub id: SessionId,
    pub account_id: AccountId,
    pub user_agent: String,
    pub created_at: NaiveDateTime,
    pub last_active_at: NaiveDateTime,
}
