use crate::config::{
    Forwarding, ForwardingTo, Ipv6Resolve, ListenAs, LoadBalanceStrategy, RelayHost, RelayProtocol,
    Remote, TcpProxyProtocol,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize};

/// Compiled ingest strategy for a listener (certs parsed once at apply time).
pub enum Ingest {
    Raw,
    Tls(openssl::ssl::SslAcceptor),
    RelayTcp,
    RelayTls(openssl::ssl::SslAcceptor),
    RelayQuic,
}

/// Compiled forwarding target tree (LB counters allocated once at apply time).
pub enum Target {
    Exit {
        destination: Remote,
        ipv6_resolve: Ipv6Resolve,
        send_pp: Option<TcpProxyProtocol>,
    },
    Relay {
        protocol: RelayProtocol,
        destination: Remote,
        ipv6_resolve: Ipv6Resolve,
        sni: Option<String>,
    },
    LoadBalance {
        members: Vec<Arc<Target>>,
        strategy: LoadBalanceStrategy,
        next: AtomicUsize,
        rng: AtomicU64,
    },
}

/// A ready-to-serve compiled form of one forwarding role.
pub struct PreparedForwarding {
    pub forwarding: Arc<Forwarding>,
    pub ingest: Ingest,
    pub target: Arc<Target>,
    pub quic_server: Option<quinn::ServerConfig>,
}

impl PreparedForwarding {
    pub fn build(
        f: &Forwarding,
        ipv6_resolve: Ipv6Resolve,
    ) -> Result<PreparedForwarding, crate::BoxError> {
        let ingest = match &f.listen_as {
            ListenAs::Raw => Ingest::Raw,
            ListenAs::Tls(c) => Ingest::Tls(crate::tls::server_acceptor(c)?),
            ListenAs::Relay(RelayHost::Tcp) => Ingest::RelayTcp,
            ListenAs::Relay(RelayHost::TlsOverTcp(c)) => {
                Ingest::RelayTls(crate::tls::server_acceptor(c)?)
            }
            ListenAs::Relay(RelayHost::Quic(_)) => Ingest::RelayQuic,
        };
        let quic_server = match &f.listen_as {
            ListenAs::Relay(RelayHost::Quic(c)) => Some(crate::tls::quic_server_config(c)?),
            _ => None,
        };
        let target = compile_target(&f.to, f.send_proxy_protocol, ipv6_resolve);
        Ok(PreparedForwarding {
            forwarding: Arc::new(f.clone()),
            ingest,
            target,
            quic_server,
        })
    }
}

fn compile_target(
    to: &ForwardingTo,
    send_pp: Option<TcpProxyProtocol>,
    ipv6_resolve: Ipv6Resolve,
) -> Arc<Target> {
    match to {
        ForwardingTo::Exit { destination } => Arc::new(Target::Exit {
            destination: destination.clone(),
            ipv6_resolve,
            send_pp,
        }),
        ForwardingTo::Relay {
            protocol,
            destination,
            sni,
        } => Arc::new(Target::Relay {
            protocol: *protocol,
            destination: destination.clone(),
            ipv6_resolve,
            sni: sni.clone(),
        }),
        ForwardingTo::LoadBalance(g) => {
            let members = g
                .members
                .iter()
                .map(|m| compile_target(m, send_pp, ipv6_resolve))
                .collect();
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9E37_79B9_7F4A_7C15)
                | 1;
            Arc::new(Target::LoadBalance {
                members,
                strategy: g.strategy,
                next: AtomicUsize::new(0),
                rng: AtomicU64::new(seed),
            })
        }
    }
}
