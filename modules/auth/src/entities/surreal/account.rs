use kanau::processor::Processor;
use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;
use wakuwaku::surreal::SurrealProcessor;

table_record!(AccountId, "auth_account");

#[derive(Debug, Clone, SurrealValue)]
pub struct AccountEntity {
    pub id: AccountId,
    pub email: String,
    pub password_hash: String,
    pub role: AccountRole,
}

/// Account roles. `#[surreal(untagged, rename_all = "snake_case")]` makes this
/// encode as a plain string (`"admin"`/`"maintainer"`/`"observer"`) so the
/// `role` column is a simple string with a string-literal `Kind`, rather than
/// the default tagged-object encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum AccountRole {
    Admin,
    Maintainer,
    Observer,
}

pub struct FindAccountByEmail<'a> {
    pub email: &'a str,
}

impl<'a> Processor<FindAccountByEmail<'a>> for SurrealProcessor {
    type Output = Option<AccountEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindAccountByEmail", skip_all, err)]
    async fn process(&self, input: FindAccountByEmail<'a>) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM auth_account WHERE email = $email LIMIT 1")
            .bind(("email", input.email.to_owned()))
            .await?;
        resp.take::<Option<AccountEntity>>(0)
    }
}

pub struct CreateAccount {
    pub email: String,
    pub password_hash: String,
    pub role: AccountRole,
}

impl Processor<CreateAccount> for SurrealProcessor {
    type Output = AccountEntity;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:CreateAccount", skip_all, err)]
    async fn process(&self, input: CreateAccount) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query(
                "CREATE auth_account CONTENT { email: $email, password_hash: $password_hash, role: $role }",
            )
            .bind(("email", input.email))
            .bind(("password_hash", input.password_hash))
            .bind(("role", input.role))
            .await?;
        resp.take::<Option<AccountEntity>>(0)?.ok_or_else(|| {
            surrealdb::Error::internal("create auth_account returned no row".to_string())
        })
    }
}

pub struct UpdateAccountPassword {
    pub id: AccountId,
    pub password_hash: String,
}

impl Processor<UpdateAccountPassword> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateAccountPassword", skip_all, err)]
    async fn process(&self, input: UpdateAccountPassword) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $id SET password_hash = $password_hash")
            .bind(("id", input.id))
            .bind(("password_hash", input.password_hash))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct UpdateAccountEmail {
    pub id: AccountId,
    pub new_email: String,
}

impl Processor<UpdateAccountEmail> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateAccountEmail", skip_all, err)]
    async fn process(&self, input: UpdateAccountEmail) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $id SET email = $new_email")
            .bind(("id", input.id))
            .bind(("new_email", input.new_email))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct FindAccountById {
    pub id: AccountId,
}

impl Processor<FindAccountById> for SurrealProcessor {
    type Output = Option<AccountEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:FindAccountById", skip_all, err)]
    async fn process(&self, input: FindAccountById) -> Result<Self::Output, Self::Error> {
        let mut resp = self
            .db()
            .query("SELECT * FROM $id")
            .bind(("id", input.id))
            .await?;
        resp.take::<Option<AccountEntity>>(0)
    }
}

pub struct ListAccounts;

impl Processor<ListAccounts> for SurrealProcessor {
    type Output = Vec<AccountEntity>;
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:ListAccounts", skip_all, err)]
    async fn process(&self, _input: ListAccounts) -> Result<Self::Output, Self::Error> {
        let mut resp = self.db().query("SELECT * FROM auth_account").await?;
        resp.take::<Vec<AccountEntity>>(0)
    }
}

pub struct UpdateAccountRole {
    pub id: AccountId,
    pub role: AccountRole,
}

impl Processor<UpdateAccountRole> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:UpdateAccountRole", skip_all, err)]
    async fn process(&self, input: UpdateAccountRole) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("UPDATE $id SET role = $role")
            .bind(("id", input.id))
            .bind(("role", input.role))
            .await?
            .check()?;
        Ok(())
    }
}

pub struct DeleteAccount {
    pub id: AccountId,
}

impl Processor<DeleteAccount> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    #[tracing::instrument(name = "Query:DeleteAccount", skip_all, err)]
    async fn process(&self, input: DeleteAccount) -> Result<Self::Output, Self::Error> {
        self.db()
            .query("DELETE $id")
            .bind(("id", input.id))
            .await?
            .check()?;
        Ok(())
    }
}
