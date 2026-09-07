use newtype_record_id::table_record;
use surrealdb_types::SurrealValue;

table_record!(DnsProviderId, "dns_provider");

pub struct DnsProviderEntity {
    pub id: DnsProviderId,
    pub provider: DnsProvider,
    pub account_id: String,
    pub api_secret: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, SurrealValue)]
pub enum DnsProvider {
    #[surreal(rename = "cloudflare")]
    Cloudflare,
    #[surreal(rename = "vercel")]
    Vercel,
}
