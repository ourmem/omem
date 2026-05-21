//! In-memory per-user token-bucket rate limiter for sharing operations.
//!
//! Sharing has no built-in rate limit, so one user with write access can fire
//! thousands of share operations in rapid succession. This adds an opt-in
//! per-user limit: each sharing call consumes one token; a user gets `per_min`
//! tokens that refill continuously at `per_min / 60` per second, so bursts up
//! to `per_min` are allowed and then a steady rate. `per_min == 0` disables it.
//!
//! State is process-local (`Mutex<HashMap>`). A multi-replica deployment would
//! want a shared store (Redis, etc.); for single-instance omem this suffices.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::domain::error::OmemError;

struct Bucket {
    tokens: f64,
    last: Instant,
}

pub struct RateLimiter {
    per_min: u32,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new(per_min: u32) -> Self {
        Self {
            per_min,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Charge one token to `key`. Returns `Err(RateLimited)` if the bucket is
    /// empty. Always `Ok` when disabled (`per_min == 0`).
    pub fn check(&self, key: &str) -> Result<(), OmemError> {
        self.check_at(key, Instant::now())
    }

    /// Time-injectable core, for deterministic tests.
    fn check_at(&self, key: &str, now: Instant) -> Result<(), OmemError> {
        if self.per_min == 0 {
            return Ok(());
        }
        let cap = self.per_min as f64;
        let refill_per_sec = cap / 60.0;
        let mut buckets = self.buckets.lock().expect("rate limiter mutex poisoned");
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: cap,
            last: now,
        });
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_per_sec).min(cap);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(OmemError::RateLimited)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn disabled_never_limits() {
        let rl = RateLimiter::new(0);
        for _ in 0..10_000 {
            assert!(rl.check("u1").is_ok());
        }
    }

    #[test]
    fn allows_burst_then_blocks() {
        let rl = RateLimiter::new(60);
        let t0 = Instant::now();
        for _ in 0..60 {
            assert!(rl.check_at("u1", t0).is_ok());
        }
        assert!(matches!(rl.check_at("u1", t0), Err(OmemError::RateLimited)));
    }

    #[test]
    fn refills_over_time() {
        let rl = RateLimiter::new(60); // 1 token/sec
        let t0 = Instant::now();
        for _ in 0..60 {
            let _ = rl.check_at("u1", t0);
        }
        assert!(rl.check_at("u1", t0).is_err());
        let t2 = t0 + Duration::from_secs(2);
        assert!(rl.check_at("u1", t2).is_ok());
        assert!(rl.check_at("u1", t2).is_ok());
        assert!(rl.check_at("u1", t2).is_err());
    }

    #[test]
    fn keys_are_independent() {
        let rl = RateLimiter::new(1);
        let t0 = Instant::now();
        assert!(rl.check_at("u1", t0).is_ok());
        assert!(rl.check_at("u1", t0).is_err());
        assert!(rl.check_at("u2", t0).is_ok());
    }
}
