use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

use crate::entities::surreal::account::AccountId;

table_record!(ApiKeyId, "api_key");

#[derive(Clone, SurrealValue)]
pub struct ApiKeyEntity {
    pub id: ApiKeyId,
    pub name: String,
    pub owner: AccountId,
    pub secret: String,
}
