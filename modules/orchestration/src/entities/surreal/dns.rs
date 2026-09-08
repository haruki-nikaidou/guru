use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

table_record!(DnsProviderId, "dns_provider");

#[derive(Debug, Clone, SurrealValue)]
pub struct DnsProviderEntity {
    pub id: DnsProviderId,
    pub provider: DnsProvider,
    pub account_id: String,
    pub api_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
#[surreal(untagged, rename_all = "snake_case")]
pub enum DnsProvider {
    Cloudflare,
    Vercel,
}
