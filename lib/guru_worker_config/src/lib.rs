#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

//! The `guru-worker` data-plane configuration model.
//!
//! Both sides of the control plane share this crate: `guru-worker` deserializes the
//! TOML it receives (or loads from disk) into [`Config`], and `guru-master` derives a
//! [`Config`] from a canvas and serializes it back to TOML.

use compact_str::CompactString;
use serde::Deserializer;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Everything that can go wrong while reading, validating or writing a config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("invalid remote '{0}': expected host:port")]
    RemoteFormat(String),
    #[error("invalid port in remote '{0}'")]
    RemotePort(String),
    #[error("duplicate listener {addr} ({tag})")]
    DuplicateListener { addr: SocketAddr, tag: String },
    #[error("forwarding {0} relay to tls/quic requires `sni`")]
    MissingSni(String),
    #[error("forwarding {0} has an empty load-balance group")]
    EmptyLoadBalance(String),
    #[error("read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("parse toml: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("serialize toml: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Forwarding {
    pub tag: String,
    pub listen: SocketAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_proxy_protocol: Option<TcpProxyProtocol>,
    pub listen_as: ListenAs,
    pub to: ForwardingTo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl serde::Serialize for Remote {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Remote::Domain(host, port) => s.collect_str(&format_args!("{host}:{port}")),
            Remote::Address(addr) => s.collect_str(addr),
        }
    }
}

impl Remote {
    /// Parses a `host:port` string. If the whole string parses as a `SocketAddr`
    /// (IPv4, or bracketed IPv6 like `[::1]:443`) it becomes `Address`; otherwise the
    /// text after the final `:` is the port and the rest is a domain name.
    pub fn parse(s: &str) -> Result<Remote, ConfigError> {
        if let Ok(addr) = s.parse::<SocketAddr>() {
            return Ok(Remote::Address(addr));
        }
        let (host, port) = s
            .rsplit_once(':')
            .ok_or_else(|| ConfigError::RemoteFormat(s.to_string()))?;
        if host.is_empty() {
            return Err(ConfigError::RemoteFormat(s.to_string()));
        }
        let port: u16 = port
            .parse()
            .map_err(|_| ConfigError::RemotePort(s.to_string()))?;
        Ok(Remote::Domain(CompactString::new(host), port))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TcpProxyProtocol {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v2")]
    V2,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TlsHostConfig {
    pub key: PathBuf,
    pub full_chain: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenAs {
    Raw,
    Tls(TlsHostConfig),
    Relay(RelayHost),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "relay_type")]
pub enum RelayHost {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    TlsOverTcp(TlsHostConfig),
    #[serde(rename = "quic")]
    Quic(TlsHostConfig),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum ForwardingTo {
    Exit {
        destination: Remote,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        send_proxy_protocol: Option<TcpProxyProtocol>,
    },
    Relay {
        protocol: RelayProtocol,
        destination: Remote,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sni: Option<String>,
    },
    LoadBalance(Box<LoadBalanceGroup>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelayProtocol {
    #[serde(rename = "tcp")]
    Tcp,
    #[serde(rename = "tls")]
    TlsOverTcp,
    #[serde(rename = "quic")]
    Quic,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LoadBalanceGroup {
    pub strategy: LoadBalanceStrategy,
    pub members: SmallVec<[ForwardingTo; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoadBalanceStrategy {
    RoundRobin,
    Random,
    IpHash,

    /// Try to connect to the first item, if it fails, try the next ones until all items are tried or success
    Fallback,
}

impl LoadBalanceGroup {
    pub fn ip_hash_somewhere(&self) -> bool {
        matches!(self.strategy, LoadBalanceStrategy::IpHash)
            || self.members.iter().any(|m| match m {
                ForwardingTo::LoadBalance(c) => c.ip_hash_somewhere(),
                _ => false,
            })
    }
    pub fn unnecessary_load_balance(&self) -> bool {
        self.members.len() == 1
    }
    pub fn empty_members(&self) -> bool {
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ipv6_resolve: Ipv6Resolve,
    #[serde(default)]
    pub log: LogConfig,
    #[serde(rename = "forwarding", default)]
    pub forwardings: Vec<Forwarding>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    /// Parses TOML text, then validates and lints it.
    pub fn from_toml_str(text: &str) -> Result<Config, ConfigError> {
        let cfg: Config = toml::from_str(text)?;
        cfg.validate()?;
        for f in &cfg.forwardings {
            f.lint();
        }
        Ok(cfg)
    }

    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Config::from_toml_str(&text)
    }

    pub fn to_toml_string(&self) -> Result<String, ConfigError> {
        Ok(toml::to_string(self)?)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut seen = HashSet::new();
        for f in &self.forwardings {
            if !seen.insert(f.listen_key()) {
                return Err(ConfigError::DuplicateListener {
                    addr: f.listen,
                    tag: f.tag.clone(),
                });
            }
            if let ForwardingTo::Relay { protocol, sni, .. } = &f.to {
                let needs = matches!(protocol, RelayProtocol::TlsOverTcp | RelayProtocol::Quic);
                if needs && sni.is_none() {
                    return Err(ConfigError::MissingSni(f.tag.clone()));
                }
            }
            reject_empty(&f.to, &f.tag)?;
        }
        Ok(())
    }
}

/// Rejects load-balance groups (including nested ones) that have no members.
fn reject_empty(to: &ForwardingTo, tag: &str) -> Result<(), ConfigError> {
    if let ForwardingTo::LoadBalance(g) = to {
        if g.empty_members() {
            return Err(ConfigError::EmptyLoadBalance(tag.to_string()));
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

    fn tls_host() -> TlsHostConfig {
        TlsHostConfig {
            key: PathBuf::from("/etc/guru/key.pem"),
            full_chain: PathBuf::from("/etc/guru/chain.pem"),
        }
    }

    #[test]
    fn config_round_trips_through_toml() {
        let cfg = Config {
            ipv6_resolve: Ipv6Resolve::Preferred,
            log: LogConfig {
                level: "debug".to_string(),
            },
            forwardings: vec![
                Forwarding {
                    tag: "raw-exit".to_string(),
                    listen: "203.0.113.10:443".parse().unwrap(),
                    receive_proxy_protocol: Some(TcpProxyProtocol::V2),
                    listen_as: ListenAs::Raw,
                    to: ForwardingTo::Exit {
                        destination: Remote::parse("10.0.0.5:8080").unwrap(),
                        send_proxy_protocol: Some(TcpProxyProtocol::V1),
                    },
                },
                Forwarding {
                    tag: "tls-relay".to_string(),
                    listen: "203.0.113.10:8443".parse().unwrap(),
                    receive_proxy_protocol: None,
                    listen_as: ListenAs::Tls(tls_host()),
                    to: ForwardingTo::Relay {
                        protocol: RelayProtocol::TlsOverTcp,
                        destination: Remote::parse("relay.internal:9000").unwrap(),
                        sni: Some("relay.example.com".to_string()),
                    },
                },
                Forwarding {
                    tag: "quic-lb".to_string(),
                    listen: "203.0.113.10:9443".parse().unwrap(),
                    receive_proxy_protocol: None,
                    listen_as: ListenAs::Relay(RelayHost::Quic(tls_host())),
                    to: ForwardingTo::LoadBalance(Box::new(LoadBalanceGroup {
                        strategy: LoadBalanceStrategy::Fallback,
                        members: smallvec::smallvec![
                            ForwardingTo::Exit {
                                destination: Remote::parse("10.0.0.6:8080").unwrap(),
                                send_proxy_protocol: None,
                            },
                            ForwardingTo::LoadBalance(Box::new(LoadBalanceGroup {
                                strategy: LoadBalanceStrategy::RoundRobin,
                                members: smallvec::smallvec![
                                    ForwardingTo::Exit {
                                        destination: Remote::parse("[2001:db8::1]:8080").unwrap(),
                                        send_proxy_protocol: None,
                                    },
                                    ForwardingTo::Exit {
                                        destination: Remote::parse("backend.internal:8080")
                                            .unwrap(),
                                        send_proxy_protocol: Some(TcpProxyProtocol::V2),
                                    },
                                ],
                            })),
                        ],
                    })),
                },
            ],
        };

        let text = cfg.to_toml_string().unwrap();
        let parsed = Config::from_toml_str(&text).unwrap();
        assert_eq!(parsed, cfg, "round-trip mismatch; emitted:\n{text}");
    }
}
