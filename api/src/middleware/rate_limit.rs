//! Per-IP token-bucket rate limiter.
//!
//! In-memory and single-process — for a multi-instance deployment this would
//! need to move to Redis, but the supported topology today is a single binary.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use salvo::prelude::*;

pub struct RateLimiter {
    quota: u32,
    window: Duration,
    state: Mutex<HashMap<IpAddr, Entry>>,
    since_sweep: Mutex<u32>,
}

/// Drop entries whose window has long since lapsed, so a stream of distinct
/// source addresses cannot grow the map without bound. Sweeping is amortised
/// over requests rather than run on a timer: the map is only ever touched while
/// a request holds the lock, so there is nothing to clean up in between.
const SWEEP_EVERY: u32 = 1024;

struct Entry {
    window_start: Instant,
    count: u32,
}

impl RateLimiter {
    pub fn new(quota: u32, window: Duration) -> Self {
        Self {
            quota,
            window,
            state: Mutex::new(HashMap::new()),
            since_sweep: Mutex::new(0),
        }
    }

    /// Whether this request should also run the eviction sweep.
    fn should_sweep(&self) -> bool {
        let mut n = match self.since_sweep.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *n += 1;
        if *n >= SWEEP_EVERY {
            *n = 0;
            true
        } else {
            false
        }
    }
}

#[async_trait]
impl Handler for RateLimiter {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let ip = client_ip(req);
        let now = Instant::now();

        let allowed = {
            let mut guard = match self.state.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let entry = guard.entry(ip).or_insert(Entry {
                window_start: now,
                count: 0,
            });
            if now.duration_since(entry.window_start) >= self.window {
                entry.window_start = now;
                entry.count = 0;
            }
            entry.count = entry.count.saturating_add(1);
            let allowed = entry.count <= self.quota;

            if self.should_sweep() {
                let window = self.window;
                guard.retain(|_, e| now.duration_since(e.window_start) < window);
            }

            allowed
        };

        if !allowed {
            res.status_code(StatusCode::TOO_MANY_REQUESTS);
            res.render(salvo::writing::Json(serde_json::json!({
                "error": { "code": "RATE_LIMITED", "message": "rate limit exceeded" }
            })));
            ctrl.skip_rest();
            return;
        }
        ctrl.call_next(req, depot, res).await;
    }
}

/// Identify the client by IP. When deployed behind a trusted reverse proxy
/// that sets `X-Forwarded-For`, the first hop in that header is preferred.
fn client_ip(req: &Request) -> IpAddr {
    if let Some(forwarded) = req.headers().get("x-forwarded-for")
        && let Ok(s) = forwarded.to_str()
        && let Some(first) = s.split(',').next()
        && let Ok(ip) = first.trim().parse::<IpAddr>()
    {
        return ip;
    }
    req.remote_addr()
        .as_ipv4()
        .map(|v| IpAddr::V4(*v.ip()))
        .or_else(|| req.remote_addr().as_ipv6().map(|v| IpAddr::V6(*v.ip())))
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED))
}
