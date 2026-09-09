//! Seamless switching: what a server may actually serve *right now*.
//!
//! Derivation ([`crate::services::derive`]) answers "what should this server serve
//! once the whole canvas has caught up". This module answers the harder question:
//! given what every other server is running at this instant, which forwardings can
//! be handed to this server without breaking a path that is still in use.
//!
//! Two rules, and they are enough:
//!
//! 1. **Never point at a listener nobody serves yet.** A forwarding whose targets
//!    are not in some server's `applied` snapshot keeps its previous shape (or is
//!    withheld if it never had one) and the server is recorded as waiting.
//! 2. **Never drop a listener somebody still points at.** A listener referenced by
//!    any other server's `desired`, `in_flight` or `applied` snapshot is kept alive
//!    from this server's own previous snapshot, even after the canvas stopped
//!    asking for it.
//!
//! Applying both on every derivation pass makes a multi-hop change converge in as
//! many passes as there are hops, with no coordinator and no ordering: each pass is
//! a pure function of the fabric's current state, so a lost message or a crashed
//! master costs a retry, never correctness.

use crate::entities::surreal::server::ServerId;
use crate::entities::surreal::topology::CanvasTopology;
use crate::entities::surreal::view::{
    ConfigSnapshot, ForwardingDeps, ListenProtocol, ListenerCap, ServerConfigViewEntity,
};
use crate::services::OrchestrationError;
use crate::services::derive::{DerivedConfig, derive_server_config};
use crate::utils::ids::record_key;
use guru_worker_config::{Config, Forwarding};
use std::collections::{HashMap, HashSet};

/// One server's config as it may be rolled out right now.
#[derive(Debug, Clone)]
pub struct Converged {
    pub config: Config,
    /// Index-aligned with `config.forwardings`.
    pub forwardings: Vec<ForwardingDeps>,
    /// Servers this one is waiting on before it can adopt its ideal config.
    pub waiting_for: Vec<ServerId>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConvergeError {
    #[error(
        "listener {ip}:{port} is still referenced as {old:?} and cannot become {new:?}; change the port"
    )]
    ListenerConflict {
        ip: String,
        port: i64,
        old: ListenProtocol,
        new: ListenProtocol,
    },
    #[error("stored snapshot is not valid TOML: {0}")]
    Snapshot(#[from] guru_worker_config::ConfigError),
}

/// Merges a server's ideal config with what the fabric can support today. The
/// server is identified by `own.server`.
pub fn converge(
    ideal: DerivedConfig,
    own: &ServerConfigViewEntity,
    views: &[ServerConfigViewEntity],
    server_of_ip: &HashMap<String, ServerId>,
) -> Result<Converged, ConvergeError> {
    let mut served_now: HashSet<ListenerCap> = HashSet::new();
    for view in views {
        if let Some(applied) = &view.applied {
            for deps in &applied.forwardings {
                served_now.insert(deps.serves.clone());
            }
        }
    }

    let own_key = record_key(&own.server.0);
    let mut referenced: HashSet<ListenerCap> = HashSet::new();
    for view in views {
        let is_own = record_key(&view.server.0) == own_key;
        let snapshots = [
            Some(&view.in_flight),
            Some(&view.applied),
            // A server's own previous `desired` must not pin its own listeners, or
            // a self-reference would never drain.
            (!is_own).then_some(&view.desired),
        ];
        for snapshot in snapshots.into_iter().flatten().flatten() {
            for deps in &snapshot.forwardings {
                for cap in &deps.points_at {
                    referenced.insert(cap.clone());
                }
            }
        }
    }

    let mut merged: Vec<(Forwarding, ForwardingDeps)> = Vec::new();
    let mut waiting: Vec<ServerId> = Vec::new();
    let mut waiting_keys: HashSet<String> = HashSet::new();

    for (forwarding, deps) in ideal.config.forwardings.into_iter().zip(ideal.forwardings) {
        let unready: Vec<&ListenerCap> = deps
            .points_at
            .iter()
            .filter(|cap| !served_now.contains(*cap))
            .collect();
        if unready.is_empty() {
            merged.push((forwarding, deps));
            continue;
        }
        for cap in unready {
            if let Some(target) = server_of_ip.get(&cap.ip) {
                let key = record_key(&target.0);
                if waiting_keys.insert(key) {
                    waiting.push(target.clone());
                }
            }
        }
        // Hold the previous shape of this listener until the target catches up.
        if let Some(previous) = old_forwarding(own, &deps.serves)? {
            merged.push(previous);
        }
    }

    // Listeners the canvas no longer asks for, but somebody still points at.
    let mut stale: Vec<&ListenerCap> = referenced
        .iter()
        .filter(|cap| {
            server_of_ip
                .get(&cap.ip)
                .map(|s| record_key(&s.0))
                .as_deref()
                == Some(own_key.as_str())
                && !merged.iter().any(|(_, deps)| &deps.serves == *cap)
        })
        .collect();
    stale.sort_by(|a, b| (&a.ip, a.port).cmp(&(&b.ip, b.port)));
    for cap in stale {
        if let Some(previous) = old_forwarding(own, cap)? {
            merged.push(previous);
        }
    }

    // `Transport` is not `Ord`, and a stable total order only needs the socket plus
    // a discriminant to separate two listeners that share one.
    merged.sort_by_key(|(f, _)| {
        (
            f.listen,
            f.transport() == guru_worker_config::Transport::Quic,
        )
    });
    for pair in merged.windows(2) {
        let (a, b) = (&pair[0].0, &pair[1].0);
        if a.listen_key() == b.listen_key() {
            return Err(ConvergeError::ListenerConflict {
                ip: pair[0].1.serves.ip.clone(),
                port: pair[0].1.serves.port,
                old: pair[0].1.serves.protocol,
                new: pair[1].1.serves.protocol,
            });
        }
    }
    waiting.sort_by_key(|s| record_key(&s.0));

    let (forwardings, deps): (Vec<_>, Vec<_>) = merged.into_iter().unzip();
    Ok(Converged {
        config: Config {
            ipv6_resolve: ideal.config.ipv6_resolve,
            log: ideal.config.log,
            forwardings,
        },
        forwardings: deps,
        waiting_for: waiting,
    })
}

/// This server's newest stored shape of one listener, preferring what it is
/// actually running over what it was merely offered.
fn old_forwarding(
    own: &ServerConfigViewEntity,
    cap: &ListenerCap,
) -> Result<Option<(Forwarding, ForwardingDeps)>, ConvergeError> {
    for snapshot in [&own.applied, &own.in_flight, &own.desired]
        .into_iter()
        .flatten()
    {
        if let Some(found) = forwarding_in(snapshot, cap)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn forwarding_in(
    snapshot: &ConfigSnapshot,
    cap: &ListenerCap,
) -> Result<Option<(Forwarding, ForwardingDeps)>, ConvergeError> {
    let Some(index) = snapshot
        .forwardings
        .iter()
        .position(|deps| &deps.serves == cap)
    else {
        return Ok(None);
    };
    let config = Config::from_toml_str(&snapshot.toml)?;
    let Some(forwarding) = config.forwardings.into_iter().nth(index) else {
        return Ok(None);
    };
    Ok(Some((forwarding, snapshot.forwardings[index].clone())))
}

/// Rejects an edit that would put a different protocol on a socket some server
/// still points at.
///
/// Such a switch has no seamless path: the two listeners cannot coexist on one
/// worker, so whichever way it is sequenced the dependants break. Rejecting it at
/// edit time turns a runtime outage into an error message that names the fix.
pub fn ensure_switch_safe(
    projected: &CanvasTopology,
    views: &[ServerConfigViewEntity],
) -> Result<(), OrchestrationError> {
    let mut referenced: Vec<ListenerCap> = Vec::new();
    for view in views {
        for snapshot in [&view.desired, &view.in_flight, &view.applied]
            .into_iter()
            .flatten()
        {
            for deps in &snapshot.forwardings {
                referenced.extend(deps.points_at.iter().cloned());
            }
        }
    }
    if referenced.is_empty() {
        return Ok(());
    }
    for server in &projected.servers {
        // A server whose config does not derive at all is reported by the
        // derivation pass, not here.
        let Ok(derived) = derive_server_config(projected, &server.id) else {
            continue;
        };
        for deps in &derived.forwardings {
            if let Some(old) = referenced.iter().find(|cap| cap.conflicts(&deps.serves)) {
                return Err(OrchestrationError::Conflict(format!(
                    "listener {}:{} is still in use as {:?}; use a new port instead of changing its protocol",
                    deps.serves.ip, deps.serves.port, old.protocol
                )));
            }
        }
    }
    Ok(())
}
