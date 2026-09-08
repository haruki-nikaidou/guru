//! Per-server signal hub plus a revision poller.
//!
//! Mutations happen in the dashboard process while streams live in the worker-facing
//! process, so an in-process broadcast alone can never observe a new revision. The
//! poller closes that gap; when AMQP fan-out lands it replaces the poller without
//! touching the hub or the stream handler.

use crate::entities::surreal::server::{ListServerWatchState, ServerWatchState};
use crate::utils::ids::record_key;
use kanau::processor::Processor;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use wakuwaku::surreal::SurrealProcessor;

const CHANNEL_CAPACITY: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSignal {
    Revision(i64),
    /// Another registration superseded the refresh key this stream authenticated with.
    Superseded,
}

struct Entry {
    tx: broadcast::Sender<AgentSignal>,
    subscribers: usize,
    last_revision: i64,
    generation: i64,
}

#[derive(Clone, Default)]
pub struct WatchHub {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

pub struct WatchSubscription {
    pub rx: broadcast::Receiver<AgentSignal>,
    server_key: String,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
}

impl Drop for WatchSubscription {
    fn drop(&mut self) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let remove = match entries.get_mut(&self.server_key) {
            Some(entry) => {
                entry.subscribers = entry.subscribers.saturating_sub(1);
                entry.subscribers == 0
            }
            None => false,
        };
        if remove {
            entries.remove(&self.server_key);
        }
    }
}

impl WatchHub {
    /// Registers interest in one server; the guard deregisters on drop.
    pub fn subscribe(
        &self,
        server_key: &str,
        generation: i64,
        last_revision: i64,
    ) -> WatchSubscription {
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = entries.entry(server_key.to_string()).or_insert_with(|| {
            let (tx, _) = broadcast::channel(CHANNEL_CAPACITY);
            Entry {
                tx,
                subscribers: 0,
                last_revision,
                generation,
            }
        });
        entry.subscribers = entry.subscribers.saturating_add(1);
        entry.generation = generation;
        entry.last_revision = entry.last_revision.max(last_revision);
        let rx = entry.tx.subscribe();
        drop(entries);
        WatchSubscription {
            rx,
            server_key: server_key.to_string(),
            entries: self.entries.clone(),
        }
    }

    /// Called by `RegisterWorker` in this process for instant supersession.
    pub fn supersede(&self, server_key: &str, generation: i64) {
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(entry) = entries.get_mut(server_key)
            && entry.generation < generation
        {
            entry.generation = generation;
            let _ = entry.tx.send(AgentSignal::Superseded);
        }
    }

    pub(crate) fn watched_servers(&self) -> Vec<crate::entities::surreal::server::ServerId> {
        let entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        entries
            .keys()
            .map(|key| crate::utils::ids::server_id(key))
            .collect()
    }

    pub(crate) fn publish(&self, state: &ServerWatchState) {
        let key = record_key(&state.id.0);
        let mut entries = match self.entries.lock() {
            Ok(entries) => entries,
            Err(poisoned) => poisoned.into_inner(),
        };
        let Some(entry) = entries.get_mut(&key) else {
            return;
        };
        if state.refresh_key_generation != entry.generation {
            entry.generation = state.refresh_key_generation;
            let _ = entry.tx.send(AgentSignal::Superseded);
            return;
        }
        if state.desired_revision > entry.last_revision {
            entry.last_revision = state.desired_revision;
            let _ = entry.tx.send(AgentSignal::Revision(state.desired_revision));
        }
    }
}

/// Polls the desired revision of every watched server until `shutdown`.
pub async fn run_poller(
    hub: WatchHub,
    db: SurrealProcessor,
    interval: Duration,
    shutdown: CancellationToken,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => return,
            _ = ticker.tick() => {}
        }
        let servers = hub.watched_servers();
        if servers.is_empty() {
            continue;
        }
        match db.process(ListServerWatchState { servers }).await {
            Ok(states) => {
                for state in &states {
                    hub.publish(state);
                }
            }
            Err(e) => tracing::error!(error = %e, "watch poll failed"),
        }
    }
}
