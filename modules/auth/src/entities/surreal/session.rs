use newtype_record_id::table_record;
use time::PrimitiveDateTime;
use super::account::AccountId;

table_record!(SessionId, "auth_session");

pub struct SessionEntity {
    pub id: SessionId,
    pub account_id: AccountId,
    pub user_agent: String,
    pub created_at: PrimitiveDateTime,
    pub last_active_at: PrimitiveDateTime,
}
