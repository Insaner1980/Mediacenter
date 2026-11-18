//! Transport admission limits, independent of native services.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub const OUTGOING_BYTES: usize = 256 * 1024;
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
pub const PING_INTERVAL: Duration = Duration::from_secs(10);
pub const PONG_TIMEOUT: Duration = Duration::from_secs(30);
pub const ACK_TIMEOUT: Duration = Duration::from_secs(5);
pub const THUMBNAIL_BYTES: usize = 512 * 1024;

pub struct CommandRate {
    at: Instant,
    tokens: f64,
    per_second: f64,
    burst: f64,
}
impl CommandRate {
    pub fn new(now: Instant) -> Self {
        Self::with_rate(now, 20.0, 40.0)
    }
    pub fn motion(now: Instant) -> Self {
        Self::with_rate(now, 30.0, 3.0)
    }
    fn with_rate(at: Instant, per_second: f64, burst: f64) -> Self {
        Self {
            at,
            tokens: burst,
            per_second,
            burst,
        }
    }
    pub fn admit(&mut self, now: Instant) -> bool {
        self.tokens = (self.tokens
            + now.saturating_duration_since(self.at).as_secs_f64() * self.per_second)
            .min(self.burst);
        self.at = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

/// Weak entries are pruned on every admission, so reconnect/device churn cannot
/// accumulate an unbounded resource map. Each in-flight job owns its permit.
#[derive(Default)]
pub struct DeviceLimits(Mutex<HashMap<String, Weak<Semaphore>>>);
impl DeviceLimits {
    pub fn acquire(&self, device: &str, maximum: usize) -> Option<OwnedSemaphorePermit> {
        let mut map = self.0.lock().ok()?;
        map.retain(|_, gate| gate.strong_count() > 0);
        let gate = map.get(device).and_then(Weak::upgrade).unwrap_or_else(|| {
            let gate = Arc::new(Semaphore::new(maximum));
            map.insert(device.into(), Arc::downgrade(&gate));
            gate
        });
        gate.try_acquire_owned().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn commands_exhaust_burst_and_refill_twenty_per_second() {
        let now = Instant::now();
        let mut rate = CommandRate::new(now);
        for _ in 0..40 {
            assert!(rate.admit(now));
        }
        assert!(!rate.admit(now));
        let later = now + Duration::from_millis(100);
        assert!(rate.admit(later));
        assert!(rate.admit(later));
        assert!(!rate.admit(later));
    }
}
