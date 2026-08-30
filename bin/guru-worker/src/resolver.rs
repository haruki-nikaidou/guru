use crate::BoxError;
use crate::config::{Ipv6Resolve, Remote};
use compact_str::CompactString;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// TTL for cached DNS resolutions.
const DNS_TTL: Duration = Duration::from_secs(300);

/// The [`Ipv6Resolve`] policy is part of the cache key, so two config generations with
/// different policies (e.g. across a `SIGHUP` reload) never share a resolved address.
type DnsCache = moka::future::Cache<(CompactString, u16, Ipv6Resolve), SocketAddr>;

static DNS_CACHE: LazyLock<DnsCache> =
    LazyLock::new(|| moka::future::Cache::builder().time_to_live(DNS_TTL).build());

/// Resolves a forwarding target to a concrete socket address. Literal addresses are
/// returned as-is; domains are resolved via the system resolver, filtered by the given
/// [`Ipv6Resolve`] policy, and cached (single-flight per key via `try_get_with`) for
/// `DNS_TTL`.
pub async fn resolve(remote: &Remote, ipv6_resolve: Ipv6Resolve) -> Result<SocketAddr, BoxError> {
    match remote {
        Remote::Address(addr) => Ok(*addr),
        Remote::Domain(host, port) => {
            let key = (host.clone(), *port, ipv6_resolve);
            DNS_CACHE
                .try_get_with(key.clone(), lookup(key.0, key.1, ipv6_resolve))
                .await
                .map_err(|e: Arc<BoxError>| format!("dns resolve failed: {e}").into())
        }
    }
}

async fn lookup(host: CompactString, port: u16, mode: Ipv6Resolve) -> Result<SocketAddr, BoxError> {
    let addrs = tokio::net::lookup_host((host.as_str(), port)).await?;
    select_addr(addrs, mode).ok_or_else(|| {
        let family = match mode {
            Ipv6Resolve::Required => "IPv6 ",
            Ipv6Resolve::Forbidden => "IPv4 ",
            Ipv6Resolve::Preferred | Ipv6Resolve::Tolerated => "",
        };
        format!("no {family}DNS records for {host}:{port}").into()
    })
}

/// Picks one address from a resolver result set according to `mode`. Returns `None` when
/// the policy cannot be satisfied (which the caller surfaces as a resolution failure).
fn select_addr(addrs: impl Iterator<Item = SocketAddr>, mode: Ipv6Resolve) -> Option<SocketAddr> {
    let mut first_v4 = None;
    let mut first_v6 = None;
    for a in addrs {
        match a {
            SocketAddr::V4(_) => first_v4 = first_v4.or(Some(a)),
            SocketAddr::V6(_) => first_v6 = first_v6.or(Some(a)),
        }
        if first_v4.is_some() && first_v6.is_some() {
            break;
        }
    }
    match mode {
        Ipv6Resolve::Required => first_v6,
        Ipv6Resolve::Preferred => first_v6.or(first_v4),
        Ipv6Resolve::Tolerated => first_v4.or(first_v6),
        Ipv6Resolve::Forbidden => first_v4,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn v4() -> SocketAddr {
        "127.0.0.1:9000".parse().unwrap()
    }

    fn v6() -> SocketAddr {
        "[::1]:9000".parse().unwrap()
    }

    #[test]
    fn required_picks_v6_when_present() {
        assert_eq!(
            select_addr([v4(), v6()].into_iter(), Ipv6Resolve::Required),
            Some(v6())
        );
    }

    #[test]
    fn required_fails_without_v6() {
        assert_eq!(select_addr([v4()].into_iter(), Ipv6Resolve::Required), None);
    }

    #[test]
    fn preferred_picks_v6_but_falls_back_to_v4() {
        assert_eq!(
            select_addr([v4(), v6()].into_iter(), Ipv6Resolve::Preferred),
            Some(v6())
        );
        assert_eq!(
            select_addr([v4()].into_iter(), Ipv6Resolve::Preferred),
            Some(v4())
        );
    }

    #[test]
    fn tolerated_picks_v4_but_falls_back_to_v6() {
        assert_eq!(
            select_addr([v6(), v4()].into_iter(), Ipv6Resolve::Tolerated),
            Some(v4())
        );
        assert_eq!(
            select_addr([v6()].into_iter(), Ipv6Resolve::Tolerated),
            Some(v6())
        );
    }

    #[test]
    fn forbidden_picks_v4_when_present() {
        assert_eq!(
            select_addr([v6(), v4()].into_iter(), Ipv6Resolve::Forbidden),
            Some(v4())
        );
    }

    #[test]
    fn forbidden_fails_without_v4() {
        assert_eq!(
            select_addr([v6()].into_iter(), Ipv6Resolve::Forbidden),
            None
        );
    }

    #[test]
    fn empty_result_is_none() {
        assert_eq!(
            select_addr(std::iter::empty(), Ipv6Resolve::Tolerated),
            None
        );
    }
}
