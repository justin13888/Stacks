//! The envelope every upstream call runs in: Stacks' own pacer for its own
//! AniList queries, the per-provider circuit breaker, the call deadline, and
//! the outcome's classification, metrics and log line.

use std::future::Future;

use tokio::time::Instant;

use super::{CameoProvider, report};
use crate::media::breaker::{BreakerState, CallOutcome, CircuitBreaker};
use crate::media::error::{MetadataError, MetadataErrorKind, Operation, UnavailableCause};
use crate::media::id::Provider;
use crate::media::pacer::Pacer;

impl CameoProvider {
    /// The breaker state of one provider.
    pub fn circuit_state(&self, provider: Provider) -> BreakerState {
        self.breaker(provider).state()
    }

    pub(super) fn breaker(&self, provider: Provider) -> &CircuitBreaker {
        match provider {
            Provider::Tmdb => &self.tmdb_breaker,
            Provider::AniList => &self.anilist_breaker,
        }
    }

    /// Run one upstream call under the breaker and the deadline, then
    /// classify, count, time and log its outcome.
    pub(super) async fn call<T, Fut>(
        &self,
        provider: Provider,
        operation: Operation,
        fut: Fut,
    ) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        self.call_with(provider, operation, None, fut).await
    }

    /// [`call`](Self::call) for one of Stacks' own AniList queries, paced by
    /// `extras_rate`.
    pub(super) async fn call_paced<T, Fut>(
        &self,
        operation: Operation,
        fut: Fut,
    ) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        self.call_with(Provider::AniList, operation, Some(&self.extras_rate), fut)
            .await
    }

    /// The body of [`call`](Self::call). With a `pacer`, the call first waits
    /// for one of its slots, before the breaker and outside the deadline: the
    /// wait is Stacks' own queue, not the provider being slow, so it must
    /// never end in a `Timeout` that trips the breaker. A slot more than the
    /// deadline away is refused at once as `RateLimited`, which leaves the
    /// breaker alone, and a slot the breaker then refuses is given back.
    async fn call_with<T, Fut>(
        &self,
        provider: Provider,
        operation: Operation,
        pacer: Option<&Pacer>,
        fut: Fut,
    ) -> Result<T, MetadataError>
    where
        Fut: Future<Output = Result<T, MetadataErrorKind>>,
    {
        let breaker = self.breaker(provider);
        let start = Instant::now();
        let result = 'call: {
            if pacer.is_some() && breaker.state() == BreakerState::Open {
                // Waiting for a slot only to be refused would be wasted.
                break 'call Err(MetadataErrorKind::Unavailable(
                    UnavailableCause::CircuitOpen,
                ));
            }
            let slot = match pacer {
                Some(pacer) => match pacer.acquire(self.call_deadline).await {
                    Ok(slot) => Some(slot),
                    Err(wait) => {
                        break 'call Err(MetadataErrorKind::RateLimited {
                            retry_after: Some(wait),
                        });
                    }
                },
                None => None,
            };
            if !breaker.try_acquire() {
                // Dropping the slot gives it back: nothing is sent.
                drop(slot);
                break 'call Err(MetadataErrorKind::Unavailable(
                    UnavailableCause::CircuitOpen,
                ));
            }
            if let Some(slot) = slot {
                slot.used();
            }
            let r = match tokio::time::timeout(self.call_deadline, fut).await {
                Ok(r) => r,
                Err(_) => Err(MetadataErrorKind::Unavailable(UnavailableCause::Timeout)),
            };
            breaker.record(match &r {
                Err(kind) if kind.trips_breaker() => CallOutcome::Unhealthy,
                _ => CallOutcome::Healthy,
            });
            r
        };
        let outcome = match &result {
            Ok(_) => "ok",
            Err(kind) => kind.class(),
        };
        metrics::histogram!(
            "stacks_metadata_request_duration_seconds",
            "provider" => provider.as_str(),
            "operation" => operation.as_str(),
            "outcome" => outcome,
        )
        .record(start.elapsed().as_secs_f64());
        result.map_err(|kind| {
            let err = MetadataError::new(provider, operation, kind);
            report(&err);
            err
        })
    }
}
