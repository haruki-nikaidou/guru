use crate::config::{Config, Forwarding, Transport};
use crate::prepared::PreparedForwarding;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

struct ListenerHandle {
    cfg_tx: watch::Sender<Arc<PreparedForwarding>>,
    token: CancellationToken,
}

/// Owns all running listeners keyed by `(addr, transport)` and applies config diffs.
pub struct Supervisor {
    listeners: HashMap<(SocketAddr, Transport), ListenerHandle>,
}

impl Supervisor {
    pub fn new() -> Self {
        Self {
            listeners: HashMap::new(),
        }
    }

    /// Applies a config. `strict = true` (startup) makes any bind/build failure fatal;
    /// `strict = false` (reload) logs per-listener failures and applies the rest.
    ///
    /// Removed listeners stop accepting without dropping in-flight connections; retained
    /// listeners hot-swap their config; new listeners are bound and spawned.
    pub async fn apply(&mut self, cfg: &Config, strict: bool) -> Result<(), crate::BoxError> {
        let desired: HashMap<(SocketAddr, Transport), Forwarding> = cfg
            .forwardings
            .iter()
            .map(|f| (f.listen_key(), f.clone()))
            .collect();

        // 1. Remove listeners no longer present: cancel accept loop; in-flight conns survive.
        let stale: Vec<_> = self
            .listeners
            .keys()
            .filter(|k| !desired.contains_key(k))
            .cloned()
            .collect();
        for k in stale {
            if let Some(h) = self.listeners.remove(&k) {
                h.token.cancel();
                tracing::info!(addr = ?k.0, transport = ?k.1, "listener removed");
            }
        }

        // 2. Update retained: rebuild PreparedForwarding, hot-swap via watch.
        for (k, h) in self.listeners.iter() {
            if let Some(f) = desired.get(k) {
                match PreparedForwarding::build(f, cfg.ipv6_resolve) {
                    Ok(p) => {
                        let _ = h.cfg_tx.send(Arc::new(p));
                    }
                    Err(e) => {
                        tracing::error!(tag = %f.tag, error = %e, "reload build failed; keeping previous config");
                        if strict {
                            return Err(e);
                        }
                    }
                }
            }
        }

        // 3. Add new: build, bind, spawn.
        for (k, f) in desired.iter() {
            if self.listeners.contains_key(k) {
                continue;
            }
            match self.spawn_listener(f, cfg.ipv6_resolve).await {
                Ok(h) => {
                    self.listeners.insert(*k, h);
                    tracing::info!(addr = ?k.0, transport = ?k.1, tag = %f.tag, "listener started");
                }
                Err(e) => {
                    tracing::error!(tag = %f.tag, error = %e, "failed to start listener");
                    if strict {
                        return Err(e);
                    }
                }
            }
        }
        Ok(())
    }

    async fn spawn_listener(
        &self,
        f: &Forwarding,
        ipv6_resolve: crate::config::Ipv6Resolve,
    ) -> Result<ListenerHandle, crate::BoxError> {
        let prepared = Arc::new(PreparedForwarding::build(f, ipv6_resolve)?);
        let (cfg_tx, cfg_rx) = watch::channel(prepared.clone());
        let token = CancellationToken::new();
        match f.transport() {
            Transport::Tcp => {
                let l = crate::listener::bind_tcp(f.listen)?;
                tokio::spawn(crate::listener::run_tcp(l, cfg_rx, token.clone()));
            }
            Transport::Quic => {
                let sc = prepared
                    .quic_server
                    .clone()
                    .ok_or("quic listener without server config")?;
                let ep = crate::listener::bind_quic(f.listen, sc)?;
                tokio::spawn(crate::listener::run_quic(ep, cfg_rx, token.clone()));
            }
        }
        Ok(ListenerHandle { cfg_tx, token })
    }

    pub fn shutdown_all(&self) {
        for h in self.listeners.values() {
            h.token.cancel();
        }
    }
}
