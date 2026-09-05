use crate::entities::surreal::account::AccountId;
use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(ApiKeyId, "api_key");

#[derive(Clone, SurrealValue)]
pub struct ApiKeyEntity {
    pub id: ApiKeyId,
    pub name: String,
    pub owner: AccountId,
    pub secret: String,
}

pub struct CreateNewApiKey {
    pub name: String,
    pub owner: AccountId,
    pub secret: String,
}

impl Processor<CreateNewApiKey> for SurrealProcessor {
    type Output = ApiKeyId;
    type Error = surrealdb::Error;
    async fn process(&self, input: CreateNewApiKey) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct FindApiKeyBySecret {
    pub secret: String,
}

impl Processor<FindApiKeyBySecret> for SurrealProcessor {
    type Output = Option<ApiKeyEntity>;
    type Error = surrealdb::Error;
    async fn process(&self, input: FindApiKeyBySecret) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct ListApiKeysByOwner {
    pub owner: AccountId,
}

pub struct ApiKeyOmitSecret {
    pub id: ApiKeyId,
    pub name: String,
    pub owner: AccountId,
}

impl Processor<ListApiKeysByOwner> for SurrealProcessor {
    type Output = Vec<ApiKeyOmitSecret>;
    type Error = surrealdb::Error;
    async fn process(&self, input: ListApiKeysByOwner) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
