use compact_str::CompactString;
use serde::Deserializer;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Forwarding {
    pub tag: String,
    pub listen: SocketAddr,
    pub listen_as: ListenAs,
    pub to: ForwardingTo,
    pub receive_proxy_protocol: Option<TcpProxyProtocol>,
    pub send_proxy_protocol: Option<TcpProxyProtocol>,
}

#[derive(Debug, Clone)]
pub enum Remote {
    Domain(CompactString, u16),
    Address(SocketAddr),
}

impl<'de> serde::Deserialize<'de> for Remote {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = CompactString::deserialize(deserializer)?;
        Remote::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl Remote {
    /// Parses a `host:port` string. If the whole string parses as a `SocketAddr`
    /// (IPv4, or bracketed IPv6 like `[::1]:443`) it becomes `Address`; otherwise the
    /// text after the final `:` is the port and the rest is a domain name.
    fn parse(s: &str) -> Result<Remote, crate::BoxError> {
        if let Ok(addr) = s.parse::<SocketAddr>() {
            return Ok(Remote::Address(addr));
        }
        let (host, port) = s
            .rsplit_once(':')
            .ok_or_else(|| format!("invalid remote '{s}': expected host:port"))?;
        if host.is_empty() {
            return Err(format!("invalid remote '{s}': empty host").into());
        }
        let port: u16 = port
            .parse()
            .map_err(|_| format!("invalid port in remote '{s}'"))?;
        Ok(Remote::Domain(CompactString::new(host), port))
    }
}

#[derive(Debug, Clone, serde::Deserialize, Copy, PartialEq, Eq)]
pub enum TcpProxyProtocol {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v2")]
    V2,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TlsHostConfig {
    pub key: PathBuf,
    pub full_chain: PathBuf,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenAs {
    Raw,
    Tls(TlsHostConfig),
    Relay(RelayHost),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "relay_type")]
pub enum RelayHost {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    TlsOverTcp(TlsHostConfig),
    #[serde(rename = "quic")]
    Quic(TlsHostConfig),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ForwardingTo {
    Exit {
        destination: Remote,
    },
    Relay {
        protocol: RelayProtocol,
        destination: Remote,
        #[serde(default)]
        sni: Option<String>,
    },
    LoadBalance(Box<LoadBalanceGroup>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum RelayProtocol {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    TlsOverTcp,
    #[serde(rename = "quic")]
    Quic,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LoadBalanceGroup {
    pub members: SmallVec<[ForwardingTo; 4]>,
    pub strategy: LoadBalanceStrategy,
}

#[derive(Debug, Clone, serde::Deserialize, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceStrategy {
    RoundRobin,
    Random,
    IpHash,

    /// Try to connect to the first item, if it fails, try the next ones until all items are tried or success
    Fallback,
}

impl LoadBalanceGroup {
    pub(super) fn ip_hash_somewhere(&self) -> bool {
        matches!(self.strategy, LoadBalanceStrategy::IpHash)
            || self.members.iter().any(|m| match m {
                ForwardingTo::LoadBalance(c) => c.ip_hash_somewhere(),
                _ => false,
            })
    }
    pub(super) fn unnecessary_load_balance(&self) -> bool {
        self.members.len() == 1
    }
    pub(super) fn empty_members(&self) -> bool {
        self.members.is_empty()
    }
}

impl Forwarding {
    fn warn_suspicious_ip_hash(&self) {
        let sus = if self.receive_proxy_protocol.is_none() {
            match &self.to {
                ForwardingTo::Exit { .. } => false,
                ForwardingTo::Relay { .. } => false,
                ForwardingTo::LoadBalance(c) => c.ip_hash_somewhere(),
            }
        } else {
            return;
        };
        if sus {
            tracing::warn!(
                "forward role {} doesn't enable proxy protocol but used ip_hash for load balancing",
                self.tag
            );
        }
    }
    fn warn_unnecessary_load_balance(&self) {
        if let ForwardingTo::LoadBalance(c) = &self.to
            && c.unnecessary_load_balance()
        {
            tracing::warn!(
                "forward role {} has only one member in load balance group, unnecessary load balance",
                self.tag
            );
        }
    }
    fn error_empty_load_balance(&self) -> bool {
        match &self.to {
            ForwardingTo::LoadBalance(c) if c.empty_members() => {
                tracing::error!(
                    "forward role {} has no member in load balance group",
                    self.tag
                );
                true
            }
            _ => false,
        }
    }
    pub fn lint(&self) {
        if self.error_empty_load_balance() {
            return;
        }
        self.warn_suspicious_ip_hash();
        self.warn_unnecessary_load_balance();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Transport {
    Tcp,
    Quic,
}

impl Forwarding {
    pub fn transport(&self) -> Transport {
        match &self.listen_as {
            ListenAs::Relay(RelayHost::Quic(_)) => Transport::Quic,
            _ => Transport::Tcp,
        }
    }
    pub fn listen_key(&self) -> (SocketAddr, Transport) {
        (self.listen, self.transport())
    }
}

/// Global policy for choosing between IPv6 and IPv4 addresses when resolving a domain
/// destination, captured into each compiled target. `Tolerated` is the default (prefer
/// IPv4, fall back to IPv6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Ipv6Resolve {
    /// Only accept IPv6 results; if none exist, treat as a resolution failure.
    Required,
    /// When both families resolve, choose IPv6; otherwise fall back to IPv4.
    Preferred,
    /// When both families resolve, choose IPv4; otherwise fall back to IPv6.
    #[default]
    Tolerated,
    /// Only accept IPv4 results; if none exist, treat as a resolution failure.
    Forbidden,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub log: LogConfig,
    #[serde(default)]
    pub ipv6_resolve: Ipv6Resolve,
    #[serde(rename = "forwarding", default)]
    pub forwardings: Vec<Forwarding>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct LogConfig {
    pub level: String,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, crate::BoxError> {
        let text = std::fs::read_to_string(path)?;
        let cfg: Config = toml::from_str(&text)?;
        cfg.validate()?;
        for f in &cfg.forwardings {
            f.lint();
        }
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), crate::BoxError> {
        let mut seen = HashSet::new();
        for f in &self.forwardings {
            if !seen.insert(f.listen_key()) {
                return Err(format!("duplicate listener {:?} ({})", f.listen, f.tag).into());
            }
            if let ForwardingTo::Relay { protocol, sni, .. } = &f.to {
                let needs = matches!(protocol, RelayProtocol::TlsOverTcp | RelayProtocol::Quic);
                if needs && sni.is_none() {
                    return Err(
                        format!("forwarding {} relay to tls/quic requires `sni`", f.tag).into(),
                    );
                }
            }
            reject_empty(&f.to, &f.tag)?;
        }
        Ok(())
    }
}

/// Rejects load-balance groups (including nested ones) that have no members.
fn reject_empty(to: &ForwardingTo, tag: &str) -> Result<(), crate::BoxError> {
    if let ForwardingTo::LoadBalance(g) = to {
        if g.empty_members() {
            return Err(format!("forwarding {} has an empty load-balance group", tag).into());
        }
        for m in &g.members {
            reject_empty(m, tag)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn remote_parses_ipv4_socket_addr() {
        let r = Remote::parse("127.0.0.1:9000").unwrap();
        match r {
            Remote::Address(addr) => {
                assert_eq!(addr, "127.0.0.1:9000".parse::<SocketAddr>().unwrap());
            }
            other => panic!("expected Address, got {other:?}"),
        }
    }

    #[test]
    fn remote_parses_ipv6_socket_addr() {
        let r = Remote::parse("[::1]:443").unwrap();
        match r {
            Remote::Address(addr) => {
                assert_eq!(addr, "[::1]:443".parse::<SocketAddr>().unwrap());
            }
            other => panic!("expected Address, got {other:?}"),
        }
    }

    #[test]
    fn remote_parses_domain() {
        let r = Remote::parse("backend.internal:9000").unwrap();
        match r {
            Remote::Domain(host, port) => {
                assert_eq!(host, "backend.internal");
                assert_eq!(port, 9000);
            }
            other => panic!("expected Domain, got {other:?}"),
        }
    }

    #[test]
    fn remote_rejects_missing_port() {
        assert!(Remote::parse("no-port").is_err());
    }

    #[test]
    fn remote_rejects_bad_port() {
        assert!(Remote::parse("host:notaport").is_err());
    }
}
