-- Auth module schema.
--
-- Single source of truth for the `auth` module's tables. Applied in development
-- with `surrealkit sync`; the module's integration tests apply this exact file
-- against an in-memory database. Only `email` and `secret_sha256` are UNIQUE;
-- account/owner lookup indexes are ordinary (non-unique).

DEFINE TABLE auth_account SCHEMAFULL;
DEFINE FIELD email ON auth_account TYPE string;
DEFINE FIELD password_hash ON auth_account TYPE string;
DEFINE FIELD role ON auth_account TYPE string ASSERT $value INSIDE ["admin", "maintainer", "observer"];
DEFINE INDEX auth_account_email ON auth_account FIELDS email UNIQUE;

DEFINE TABLE auth_session SCHEMAFULL;
DEFINE FIELD account_id ON auth_session TYPE record<auth_account>;
DEFINE FIELD user_agent ON auth_session TYPE string;
DEFINE FIELD created_at ON auth_session TYPE datetime;
DEFINE FIELD last_active_at ON auth_session TYPE datetime;
DEFINE INDEX auth_session_account ON auth_session FIELDS account_id;

DEFINE TABLE api_key SCHEMAFULL;
DEFINE FIELD name ON api_key TYPE string;
DEFINE FIELD owner ON api_key TYPE record<auth_account>;
DEFINE FIELD secret_sha256 ON api_key TYPE string;
DEFINE FIELD created_at ON api_key TYPE datetime;
DEFINE INDEX api_key_secret ON api_key FIELDS secret_sha256 UNIQUE;
DEFINE INDEX api_key_owner ON api_key FIELDS owner;
