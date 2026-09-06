//! # `manage-tool`
//!
//! Command-line administration tool for the application. Use it for tasks that
//! live outside the request path, such as:
//!
//! - running and validating database migrations,
//! - seeding default configuration into the database,
//! - creating and managing administrator accounts,
//! - one-off maintenance and data-fix commands.
//!
//! See `bin/manage-tool/README.md` for the full description.

use auth::entities::surreal::account::{AccountRole, CreateAccount, FindAccountByEmail};
use auth::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use clap::{Parser, Subcommand};
use kanau::processor::Processor;
use surrealdb::opt::auth::Root;
use surrealdb::types::ToSql;
use wakuwaku::surreal::SurrealProcessor;

/// Administration CLI.
#[derive(Debug, Parser)]
#[command(name = "manage-tool", about = "Administration tasks for the platform")]
struct Cli {
    /// SurrealDB address (e.g. `ws://127.0.0.1:8000`).
    #[arg(long, env = "SURREALDB_HOST", default_value = "ws://127.0.0.1:8000")]
    address: String,
    /// Root username.
    #[arg(long, env = "SURREALDB_USER", default_value = "root")]
    username: String,
    /// Root password.
    #[arg(long, env = "SURREALDB_PASSWORD", default_value = "root")]
    password: String,
    /// Namespace to operate in.
    #[arg(long, env = "SURREALDB_NAMESPACE")]
    namespace: String,
    /// Database to operate in.
    #[arg(long, env = "SURREALDB_NAME")]
    database: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Bootstrap the first administrator account.
    CreateAdmin {
        /// Administrator email address.
        #[arg(long)]
        email: String,
        /// Administrator password.
        #[arg(long)]
        password: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let db = surrealdb::engine::any::connect(&cli.address).await?;
    db.signin(Root {
        username: cli.username,
        password: cli.password,
    })
    .await?;
    db.use_ns(&cli.namespace).use_db(&cli.database).await?;

    match cli.command {
        Command::CreateAdmin { email, password } => {
            create_admin(SurrealProcessor::new(db), email, password).await
        }
    }
}

/// Create the first administrator account directly via the entity layer (no
/// acting admin exists yet, so RBAC is deliberately bypassed for bootstrap).
async fn create_admin(
    db: SurrealProcessor,
    email: String,
    password: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let email = email.trim().to_lowercase();

    if db
        .process(FindAccountByEmail { email: &email })
        .await?
        .is_some()
    {
        eprintln!("An account with email {email} already exists");
        std::process::exit(1);
    }

    let password_hash = Argon2PasswordAlgorithm::default().hash_password(&password)?;
    let account = db
        .process(CreateAccount {
            email,
            password_hash,
            role: AccountRole::Admin,
        })
        .await?;

    println!("Created admin account {}", account.id.0.to_sql());
    Ok(())
}
