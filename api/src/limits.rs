//! The rate limiter's counters.
//!
//! kynos owns the algorithm (a sliding window over two fixed ones), the 429,
//! `Retry-After` and the `RateLimit-*` fields; where the counters live is the
//! application's. Stacks runs as one process, so they live in memory. A
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
    counters: Mutex<HashMap<String, (u64, Instant)>>,
}

impl MemoryCounters {
    fn with<T>(&self, f: impl FnOnce(&mut HashMap<String, (u64, Instant)>) -> T) -> T {
        // A poisoned lock means another request panicked mid-update; the map
        // is still a valid map, and a limiter that stops counting is safer
        // than one that refuses every request.
        let mut counters = self
            .counters
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut counters)
    }
}

impl RateLimitStore for MemoryCounters {
    type Error = Infallible;

    async fn read(&self, key: &str) -> Result<u64, Self::Error> {
        let now = Instant::now();
        Ok(self.with(|counters| {
            counters
                .get(key)
                .filter(|(_, expires)| *expires > now)
                .map_or(0, |(count, _)| *count)
        }))
    }

    async fn increment(&self, key: &str, by: u64, ttl: Duration) -> Result<u64, Self::Error> {
        let now = Instant::now();
        Ok(self.with(|counters| {
            if counters.len() >= SWEEP_AT {
                counters.retain(|_, (_, expires)| *expires > now);
            }
            let entry = counters.entry(key.to_owned()).or_insert((0, now + ttl));
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
}
