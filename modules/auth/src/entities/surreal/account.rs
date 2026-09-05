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

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
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
    async fn process(&self, input: FindAccountByEmail<'a>) -> Result<Self::Output, Self::Error> {
        todo!()
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
    async fn process(&self, input: CreateAccount) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct UpdateAccountPassword {
    pub id: AccountId,
    pub new_password: String,
}

impl Processor<UpdateAccountPassword> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    async fn process(&self, input: UpdateAccountPassword) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct UpdateAccountEmail {
    pub id: AccountId,
    pub new_email: String,
}

impl Processor<UpdateAccountEmail> for SurrealProcessor {
    type Output = ();
    type Error = surrealdb::Error;
    async fn process(&self, input: UpdateAccountEmail) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}

pub struct FindAccountById {
    pub id: AccountId,
}

impl Processor<FindAccountById> for SurrealProcessor {
    type Output = Option<AccountEntity>;
    type Error = surrealdb::Error;
    async fn process(&self, input: FindAccountById) -> Result<Self::Output, Self::Error> {
        todo!()
    }
}
