use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

table_record!(AccountId, "auth_account");

#[derive(Debug, Clone, SurrealValue)]
pub struct AccountEntity {
    pub id: AccountId,
    pub email: String,
    pub password_hash: String,
    pub role: AccountRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
pub enum AccountRole {
    Admin,
    Maintainer,
    Observer,
}
