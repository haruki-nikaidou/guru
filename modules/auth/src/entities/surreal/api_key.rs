use crate::entities::surreal::account::AccountId;
use chrono::{DateTime, Utc};
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
    pub secret_sha256: String,
    pub created_at: DateTime<Utc>,
}

pub struct CreateNewApiKey {
    pub name: String,
    pub owner: AccountId,
    pub secret_sha256: String,
    pub created_at: DateTime<Utc>,
}

impl Processor<CreateNewApiKey> for SurrealProcessor {
    type Output = ApiKeyId;
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateNewApiKey) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE api_key CONTENT { name: $name, owner: $owner, secret_sha256: $secret_sha256, created_at: $created_at }",
            )
            .bind(("name", input.name))
            .bind(("owner", input.owner))
            .bind(("secret_sha256", input.secret_sha256))
            .bind(("created_at", input.created_at))
            .await?;
        resp.take::<Option<ApiKeyEntity>>(0)?
            .map(|entity| entity.id)
            .ok_or_else(|| surrealdb::Error::internal("create api_key returned no row".to_string()))
    }
}

pub struct FindApiKeyByDigest {
    pub secret_sha256: String,
}

impl Processor<FindApiKeyByDigest> for SurrealProcessor {
    type Output = Option<ApiKeyEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindApiKeyByDigest) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM api_key WHERE secret_sha256 = $secret_sha256 LIMIT 1")
            .bind(("secret_sha256", input.secret_sha256))
            .await?;
        resp.take::<Option<ApiKeyEntity>>(0)
    }
}

pub struct FindApiKeyById {
    pub id: ApiKeyId,
}

impl Processor<FindApiKeyById> for SurrealProcessor {
    type Output = Option<ApiKeyEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindApiKeyById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<ApiKeyEntity>>(0)
    }
}

pub struct ListApiKeysByOwner {
    pub owner: AccountId,
}

#[derive(Clone, SurrealValue)]
pub struct ApiKeyOmitSecret {
    pub id: ApiKeyId,
    pub name: String,
    pub owner: AccountId,
    pub created_at: DateTime<Utc>,
}

impl Processor<ListApiKeysByOwner> for SurrealProcessor {
    type Output = Vec<ApiKeyOmitSecret>;
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ListApiKeysByOwner) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT id, name, owner, created_at FROM api_key WHERE owner = $owner")
            .bind(("owner", input.owner))
            .await?;
        resp.take::<Vec<ApiKeyOmitSecret>>(0)
    }
}

pub struct DeleteApiKey {
    pub id: ApiKeyId,
}

impl Processor<DeleteApiKey> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: DeleteApiKey) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE $id")
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct DeleteApiKeysByOwner {
    pub owner: AccountId,
}

impl Processor<DeleteApiKeysByOwner> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: DeleteApiKeysByOwner) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE api_key WHERE owner = $owner")
            .bind(("owner", input.owner))
            .await?
            .check()?;
        Ok(())
    }
}
