use crate::BoxError;
use crate::pipe::{TargetStream, connect_target};
use crate::prepared::Target;
use guru_worker_config::LoadBalanceStrategy;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Selects a member of a load-balance group per the strategy and connects to it.
pub async fn connect_balanced(
    members: &[Arc<Target>],
    strategy: LoadBalanceStrategy,
    next: &AtomicUsize,
    rng: &AtomicU64,
    client_addr: SocketAddr,
) -> Result<TargetStream, BoxError> {
    if members.is_empty() {
        return Err("empty load-balance group".into());
    }
    match strategy {
        LoadBalanceStrategy::RoundRobin => {
            // The counter is meant to wrap; `members` is non-empty, so the fallback
            // index is unreachable.
            let i = next
                .fetch_add(1, Ordering::Relaxed)
                .checked_rem(members.len())
                .unwrap_or(0);
            Box::pin(connect_target(&members[i], client_addr)).await
        }
        LoadBalanceStrategy::Random => {
            let i = (xorshift(rng) as usize)
                .checked_rem(members.len())
                .unwrap_or(0);
            Box::pin(connect_target(&members[i], client_addr)).await
        }
        LoadBalanceStrategy::IpHash => {
            let mut h = DefaultHasher::new();
            client_addr.ip().hash(&mut h);
            let i = (h.finish() as usize)
                .checked_rem(members.len())
                .unwrap_or(0);
            Box::pin(connect_target(&members[i], client_addr)).await
        }
        LoadBalanceStrategy::Fallback => {
            let mut last: Option<BoxError> = None;
            for m in members {
                match Box::pin(connect_target(m, client_addr)).await {
                    Ok(s) => return Ok(s),
                    Err(e) => last = Some(e),
                }
            }
            Err(last.unwrap_or_else(|| "no load-balance members".into()))
        }
    }
}

fn xorshift(state: &AtomicU64) -> u64 {
    let mut x = state.load(Ordering::Relaxed);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    state.store(x, Ordering::Relaxed);
    x
}
