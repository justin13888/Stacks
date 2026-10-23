//! A per-provider circuit breaker.
//!
//! After `failure_threshold` consecutive health failures (transport errors,
//! 5xx, timeouts) the circuit opens and calls fail fast with
//! `Unavailable(CircuitOpen)` instead of queueing behind a dead upstream.
//! After `cooldown` one probe call is let through (half-open): success closes
//! the circuit, another health failure re-opens it for a further cooldown.
//!
//! The state is exported as the `stacks_metadata_circuit_state{provider}`
//! gauge: 0 closed, 1 half-open, 2 open.
//!
//! Time is read through [`tokio::time::Instant`], so tests can drive it with
//! a paused clock.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::id::Provider;

/// Breaker tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakerConfig {
    /// Consecutive health failures that open the circuit.
    pub failure_threshold: u32,
    /// How long the circuit stays open before a probe is allowed.
    pub cooldown: Duration,
}

impl Default for BreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            cooldown: Duration::from_secs(30),
        }
    }
}

/// The observable breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    HalfOpen,
    Open,
}

impl BreakerState {
    fn gauge_value(self) -> f64 {
        match self {
            BreakerState::Closed => 0.0,
            BreakerState::HalfOpen => 1.0,
            BreakerState::Open => 2.0,
        }
    }
}

#[derive(Debug)]
enum Inner {
    Closed {
        failures: u32,
    },
    Open {
        until: Instant,
    },
    /// One probe is in flight since `since`. A probe that never reports back
    /// (its future was dropped) stops blocking others after a cooldown.
    HalfOpen {
        since: Instant,
    },
}

/// How a finished call should be counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallOutcome {
    /// The provider answered (including a not-found or a rejected request).
    Healthy,
    /// The provider is unhealthy: transport failure, 5xx, timeout.
    Unhealthy,
}

/// A circuit breaker for one provider.
#[derive(Debug)]
pub struct CircuitBreaker {
    provider: Provider,
    config: BreakerConfig,
    inner: Mutex<Inner>,
}

impl CircuitBreaker {
    pub fn new(provider: Provider, config: BreakerConfig) -> Self {
        let breaker = Self {
            provider,
            config: BreakerConfig {
                failure_threshold: config.failure_threshold.max(1),
                cooldown: config.cooldown,
            },
            inner: Mutex::new(Inner::Closed { failures: 0 }),
        };
        breaker.publish(BreakerState::Closed);
        breaker
    }

    /// The current state, without changing it.
    pub fn state(&self) -> BreakerState {
        match *self.lock() {
            Inner::Closed { .. } => BreakerState::Closed,
            Inner::Open { until } if Instant::now() >= until => BreakerState::HalfOpen,
            Inner::Open { .. } => BreakerState::Open,
            Inner::HalfOpen { .. } => BreakerState::HalfOpen,
        }
    }

    /// Whether a call may go out now. An open circuit whose cooldown has
    /// passed admits exactly one probe.
    pub fn try_acquire(&self) -> bool {
        let now = Instant::now();
        let mut inner = self.lock();
        match *inner {
            Inner::Closed { .. } => true,
            Inner::Open { until } if now >= until => {
                *inner = Inner::HalfOpen { since: now };
                drop(inner);
                self.publish(BreakerState::HalfOpen);
                true
            }
            Inner::Open { .. } => false,
            Inner::HalfOpen { since } if now.duration_since(since) >= self.config.cooldown => {
                *inner = Inner::HalfOpen { since: now };
                true
            }
            Inner::HalfOpen { .. } => false,
        }
    }

    /// Record how an admitted call ended.
    pub fn record(&self, outcome: CallOutcome) {
        let now = Instant::now();
        let mut inner = self.lock();
        let next = match (&*inner, outcome) {
            (_, CallOutcome::Healthy) => Inner::Closed { failures: 0 },
            (Inner::Closed { failures }, CallOutcome::Unhealthy) => {
                let failures = failures.saturating_add(1);
                if failures >= self.config.failure_threshold {
                    Inner::Open {
                        until: now + self.config.cooldown,
                    }
                } else {
                    Inner::Closed { failures }
                }
            }
            // A failed probe, or a straggler finishing while open.
            (Inner::HalfOpen { .. } | Inner::Open { .. }, CallOutcome::Unhealthy) => Inner::Open {
                until: now + self.config.cooldown,
            },
        };
        let state = match next {
            Inner::Closed { .. } => BreakerState::Closed,
            Inner::Open { .. } => BreakerState::Open,
            Inner::HalfOpen { .. } => BreakerState::HalfOpen,
        };
        let changed = std::mem::discriminant(&*inner) != std::mem::discriminant(&next);
        *inner = next;
        drop(inner);
        if changed {
            if state == BreakerState::Open {
                tracing::warn!(provider = %self.provider, "metadata circuit opened");
            } else if state == BreakerState::Closed {
                tracing::info!(provider = %self.provider, "metadata circuit closed");
            }
            self.publish(state);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        // The state is a plain value that is always written whole, so a
        // panic elsewhere cannot leave it half-updated.
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn publish(&self, state: BreakerState) {
        metrics::gauge!("stacks_metadata_circuit_state", "provider" => self.provider.as_str())
            .set(state.gauge_value());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaker(threshold: u32) -> CircuitBreaker {
        CircuitBreaker::new(
            Provider::Tmdb,
            BreakerConfig {
                failure_threshold: threshold,
                cooldown: Duration::from_secs(10),
            },
        )
    }

    #[tokio::test(start_paused = true)]
    async fn opens_after_threshold_consecutive_failures() {
        let b = breaker(3);
        for _ in 0..2 {
            assert!(b.try_acquire());
            b.record(CallOutcome::Unhealthy);
        }
        assert_eq!(b.state(), BreakerState::Closed);
        assert!(b.try_acquire());
        b.record(CallOutcome::Unhealthy);
        assert_eq!(b.state(), BreakerState::Open);
        assert!(!b.try_acquire());
    }

    #[tokio::test(start_paused = true)]
    async fn a_success_resets_the_count() {
        let b = breaker(2);
        b.record(CallOutcome::Unhealthy);
        b.record(CallOutcome::Healthy);
        b.record(CallOutcome::Unhealthy);
        assert_eq!(b.state(), BreakerState::Closed);
    }

    #[tokio::test(start_paused = true)]
    async fn half_open_admits_one_probe_and_recovers() {
        let b = breaker(1);
        b.record(CallOutcome::Unhealthy);
        assert!(!b.try_acquire());
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(b.state(), BreakerState::HalfOpen);
        assert!(b.try_acquire(), "the probe is admitted");
        assert!(!b.try_acquire(), "only one probe at a time");
        b.record(CallOutcome::Healthy);
        assert_eq!(b.state(), BreakerState::Closed);
        assert!(b.try_acquire());
    }

    #[tokio::test(start_paused = true)]
    async fn a_failed_probe_reopens() {
        let b = breaker(1);
        b.record(CallOutcome::Unhealthy);
        tokio::time::advance(Duration::from_secs(10)).await;
        assert!(b.try_acquire());
        b.record(CallOutcome::Unhealthy);
        assert_eq!(b.state(), BreakerState::Open);
        assert!(!b.try_acquire());
    }

    #[tokio::test(start_paused = true)]
    async fn an_abandoned_probe_does_not_wedge_the_breaker() {
        let b = breaker(1);
        b.record(CallOutcome::Unhealthy);
        tokio::time::advance(Duration::from_secs(10)).await;
        assert!(b.try_acquire());
        // The probe's future is dropped and never records an outcome.
        tokio::time::advance(Duration::from_secs(10)).await;
        assert!(b.try_acquire(), "a new probe after another cooldown");
    }
}
