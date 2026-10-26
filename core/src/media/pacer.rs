//! A token bucket for the metadata adapter's own requests, which cameo's rate
//! limiter does not see.

use std::time::Duration;

use tokio::time::Instant;

use super::provider::RateSettings;

/// A token bucket: up to `max_requests` at once, refilled at
/// `max_requests` per `per`. Each caller reserves its slot before waiting,
/// so concurrent callers are served in order.
pub(super) struct Pacer {
    /// Tokens left (negative while callers wait for reserved slots), and
    /// when they were counted.
    state: std::sync::Mutex<(f64, Instant)>,
    capacity: f64,
    per_token: Duration,
}

impl Pacer {
    pub(super) fn new(rate: RateSettings) -> Self {
        let max = rate.max_requests.max(1);
        Self {
            state: std::sync::Mutex::new((f64::from(max), Instant::now())),
            capacity: f64::from(max),
            per_token: rate.per / max,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, (f64, Instant)> {
        // The state is a plain value that is always written whole, so a
        // panic elsewhere cannot leave it half-updated.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Reserve a slot at most `max_wait` away and wait for it. A slot further
    /// away than that is refused with how long it would have taken, and
    /// nothing is reserved. The returned [`Slot`] gives the token back when
    /// dropped unless it is [`used`](Slot::used), and so does dropping this
    /// future while it waits: a cancelled caller leaves no backlog behind.
    pub(super) async fn acquire(&self, max_wait: Duration) -> Result<Slot<'_>, Duration> {
        let wait = {
            let mut state = self.lock();
            let now = Instant::now();
            let refilled = if self.per_token.is_zero() {
                self.capacity
            } else {
                now.duration_since(state.1).as_secs_f64() / self.per_token.as_secs_f64()
            };
            let left = (state.0 + refilled).min(self.capacity) - 1.0;
            let wait = if left >= 0.0 {
                Duration::ZERO
            } else {
                self.per_token.mul_f64(-left)
            };
            if wait > max_wait {
                return Err(wait);
            }
            *state = (left, now);
            wait
        };
        let slot = Slot {
            pacer: self,
            used: false,
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        Ok(slot)
    }

    /// Give back a reserved token that was never used.
    fn refund(&self) {
        let mut state = self.lock();
        state.0 = (state.0 + 1.0).min(self.capacity);
    }
}

/// A token reserved from a [`Pacer`]. Dropped without [`used`](Slot::used),
/// it is returned.
pub(super) struct Slot<'a> {
    pacer: &'a Pacer,
    used: bool,
}

impl Slot<'_> {
    /// The request this slot was reserved for is being sent: keep the token.
    pub(super) fn used(mut self) {
        self.used = true;
    }
}

impl Drop for Slot<'_> {
    fn drop(&mut self) {
        if !self.used {
            self.pacer.refund();
        }
    }
}

#[cfg(test)]
mod tests {
    use futures::FutureExt;

    use super::*;

    /// One token, refilled every 10 seconds.
    fn slow_pacer() -> Pacer {
        Pacer::new(RateSettings {
            max_requests: 1,
            per: Duration::from_secs(10),
        })
    }

    /// How many whole seconds away the next slot is, without reserving it.
    fn next_wait(p: &Pacer) -> u64 {
        match p.acquire(Duration::ZERO).now_or_never() {
            Some(Ok(_slot)) => 0,
            Some(Err(wait)) => wait.as_secs_f64().round() as u64,
            None => unreachable!("a zero bound never sleeps"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_waiter_cancelled_mid_wait_gives_its_slot_back() {
        let p = slow_pacer();
        p.acquire(Duration::MAX).await.expect("first slot").used();
        // The second caller reserves the slot 10s away, then is cancelled
        // (as `call`'s deadline or a dropped request would) after 1s.
        let cancelled = tokio::time::timeout(Duration::from_secs(1), p.acquire(Duration::MAX));
        assert!(cancelled.await.is_err(), "the wait outlasts the timeout");
        // Its reservation is gone: the next slot is the one it gave back,
        // 9s away, not one behind it 19s away.
        assert_eq!(next_wait(&p), 9);
    }

    #[tokio::test(start_paused = true)]
    async fn a_slot_dropped_unused_is_given_back() {
        let p = slow_pacer();
        drop(p.acquire(Duration::MAX).await.expect("first slot"));
        assert_eq!(next_wait(&p), 0);
        p.acquire(Duration::MAX).await.expect("again").used();
        assert_eq!(next_wait(&p), 10);
    }

    #[tokio::test(start_paused = true)]
    async fn a_slot_beyond_the_bound_is_refused_without_reserving() {
        let p = slow_pacer();
        p.acquire(Duration::MAX).await.expect("first slot").used();
        let refused = p.acquire(Duration::from_secs(5)).await;
        assert!(refused.is_err(), "a slot 10s away is beyond a 5s bound");
        // Nothing was reserved, so the backlog did not grow.
        assert_eq!(next_wait(&p), 10);
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(next_wait(&p), 0);
    }
}
