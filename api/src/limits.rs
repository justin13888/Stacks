//! The rate limiter's counters.
//!
//! kynos owns the algorithm (a sliding window over two fixed ones), the 429,
//! `Retry-After` and the `X-RateLimit-*` fields; where the counters live is
//! the application's. Stacks runs as one process, so they live in memory. A
//! deployment of several API replicas would move them to a shared store
//! behind the same trait.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use kynos::middleware::rate_limit::store::RateLimitStore;

/// How many distinct counters are kept before expired ones are swept.
const SWEEP_AT: usize = 10_000;

/// Counters keyed by client and window, each expiring a TTL after creation.
#[derive(Default)]
pub struct MemoryCounters {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    counters: HashMap<String, (u64, Instant)>,
    /// When the next sweep may run. A sweep scans every counter, so past
    /// [`SWEEP_AT`] it runs at most once per TTL rather than on every request.
    next_sweep: Option<Instant>,
}

impl MemoryCounters {
    fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        // A poisoned lock means another request panicked mid-update; the map
        // is still a valid map, and a limiter that stops counting is safer
        // than one that refuses every request.
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut state)
    }
}

impl State {
    /// Drops expired counters once there are many, at most once per `ttl`.
    fn sweep(&mut self, now: Instant, ttl: Duration) {
        if self.counters.len() < SWEEP_AT || self.next_sweep.is_some_and(|next| now < next) {
            return;
        }
        self.counters.retain(|_, (_, expires)| *expires > now);
        self.next_sweep = Some(now + ttl);
    }
}

impl RateLimitStore for MemoryCounters {
    type Error = Infallible;

    async fn read(&self, key: &str) -> Result<u64, Self::Error> {
        let now = Instant::now();
        Ok(self.with(|state| {
            state
                .counters
                .get(key)
                .filter(|(_, expires)| *expires > now)
                .map_or(0, |(count, _)| *count)
        }))
    }

    async fn increment(&self, key: &str, by: u64, ttl: Duration) -> Result<u64, Self::Error> {
        let now = Instant::now();
        Ok(self.with(|state| {
            state.sweep(now, ttl);
            let entry = state
                .counters
                .entry(key.to_owned())
                .or_insert((0, now + ttl));
            if entry.1 <= now {
                *entry = (0, now + ttl);
            }
            entry.0 = entry.0.saturating_add(by);
            entry.0
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn counters_add_up_and_expire() {
        let store = MemoryCounters::default();
        assert_eq!(store.read("a").await, Ok(0));
        assert_eq!(
            store.increment("a", 2, Duration::from_secs(60)).await,
            Ok(2)
        );
        assert_eq!(
            store.increment("a", 1, Duration::from_secs(60)).await,
            Ok(3)
        );
        assert_eq!(store.read("a").await, Ok(3));
        assert_eq!(store.increment("b", 1, Duration::ZERO).await, Ok(1));
        // A zero TTL expires at once.
        assert_eq!(store.read("b").await, Ok(0));
        assert_eq!(
            store.increment("b", 1, Duration::from_secs(60)).await,
            Ok(1)
        );
    }

    /// Fill the store with `n` counters that have already expired.
    fn expired(store: &MemoryCounters, prefix: &str, n: usize) {
        let past = Instant::now();
        store.with(|state| {
            for i in 0..n {
                state.counters.insert(format!("{prefix}{i}"), (1, past));
            }
        });
    }

    fn len(store: &MemoryCounters) -> usize {
        store.with(|state| state.counters.len())
    }

    #[tokio::test]
    async fn expired_counters_are_swept_at_most_once_per_ttl() {
        let store = MemoryCounters::default();
        let ttl = Duration::from_secs(60);
        expired(&store, "old", SWEEP_AT);
        store.increment("live", 1, ttl).await.unwrap();
        assert_eq!(len(&store), 1, "the first request past the cap sweeps");

        // Within the TTL another full map is not scanned again.
        expired(&store, "new", SWEEP_AT);
        store.increment("other", 1, ttl).await.unwrap();
        assert_eq!(len(&store), SWEEP_AT + 2, "no second sweep inside the TTL");

        // Once the TTL has passed, the next request sweeps again.
        store.with(|state| state.next_sweep = Some(Instant::now()));
        store.increment("later", 1, ttl).await.unwrap();
        assert_eq!(len(&store), 3, "live, other and later survive");
    }
}
