use crate::prepared::PreparedForwarding;
use guru_worker_config::{Config, Forwarding, Transport};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

struct ListenerHandle {
    cfg_tx: watch::Sender<Arc<PreparedForwarding>>,
    token: CancellationToken,
}

/// A forwarding that failed to prepare, named by its tag.
#[derive(Debug)]
pub struct ApplyError {
    pub tag: String,
    pub error: crate::BoxError,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.tag, self.error)
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.error.as_ref())
    }
}

enum PendingSocket {
    Tcp(tokio::net::TcpListener),
    Quic(quinn::Endpoint),
}

struct Pending {
    key: (SocketAddr, Transport),
    prepared: Arc<PreparedForwarding>,
    socket: PendingSocket,
}

/// Owns all running listeners keyed by `(addr, transport)` and applies config diffs.
pub struct Supervisor {
    listeners: HashMap<(SocketAddr, Transport), ListenerHandle>,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
        }
    }

    /// Applies a config all-or-nothing.
    ///
    /// Everything fallible — compiling each forwarding and binding every new socket —
    /// happens before anything is mutated, so a failure leaves the running listeners
    /// exactly as they were and releases the sockets bound during the attempt.
    ///
    /// On success: removed listeners stop accepting without dropping in-flight
    /// connections, retained listeners hot-swap their config, new listeners are spawned.
    pub fn apply(&mut self, cfg: &Config) -> Result<(), ApplyError> {
        let desired: HashMap<(SocketAddr, Transport), Forwarding> = cfg
            .forwardings
            .iter()
            .map(|f| (f.listen_key(), f.clone()))
            .collect();

        // --- prepare: nothing below mutates `self` ---
        let mut retained: Vec<((SocketAddr, Transport), Arc<PreparedForwarding>)> = Vec::new();
        let mut pending: Vec<Pending> = Vec::new();
        for (key, f) in desired.iter() {
            let prepared = Arc::new(PreparedForwarding::build(f, cfg.ipv6_resolve).map_err(
                |error| ApplyError {
                    tag: f.tag.clone(),
                    error,
                },
            )?);
            if self.listeners.contains_key(key) {
                retained.push((*key, prepared));
                continue;
            }
            let socket = match f.transport() {
                Transport::Tcp => crate::listener::bind_tcp(f.listen)
                    .map(PendingSocket::Tcp)
                    .map_err(|error| ApplyError {
                        tag: f.tag.clone(),
                        error,
                    })?,
                Transport::Quic => {
                    let sc = prepared.quic_server.clone().ok_or_else(|| ApplyError {
                        tag: f.tag.clone(),
                        error: "quic listener without server config".into(),
                    })?;
                    crate::listener::bind_quic(f.listen, sc)
                        .map(PendingSocket::Quic)
                        .map_err(|error| ApplyError {
                            tag: f.tag.clone(),
                            error,
                        })?
                }
            };
            pending.push(Pending {
                key: *key,
                prepared,
                socket,
            });
        }

        // --- commit: no fallible I/O below ---
        let stale: Vec<_> = self
            .listeners
            .keys()
            .filter(|k| !desired.contains_key(*k))
            .cloned()
            .collect();
        for k in stale {
            if let Some(h) = self.listeners.remove(&k) {
                h.token.cancel();
                tracing::info!(addr = ?k.0, transport = ?k.1, "listener removed");
            }
        }
        for (key, prepared) in retained {
            if let Some(h) = self.listeners.get(&key) {
                let _ = h.cfg_tx.send(prepared);
            }
        }
        for Pending {
            key,
            prepared,
            socket,
        } in pending
        {
            let tag = prepared.forwarding.tag.clone();
            let (cfg_tx, cfg_rx) = watch::channel(prepared);
            let token = CancellationToken::new();
            match socket {
                PendingSocket::Tcp(l) => {
                    tokio::spawn(crate::listener::run_tcp(l, cfg_rx, token.clone()));
                }
                PendingSocket::Quic(ep) => {
                    tokio::spawn(crate::listener::run_quic(ep, cfg_rx, token.clone()));
                }
            }
            self.listeners.insert(key, ListenerHandle { cfg_tx, token });
            tracing::info!(addr = ?key.0, transport = ?key.1, tag = %tag, "listener started");
        }
        Ok(())
    }

    pub fn shutdown_all(&self) {
        for h in self.listeners.values() {
            h.token.cancel();
        }
    }
}
