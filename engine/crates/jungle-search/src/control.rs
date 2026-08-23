use std::time::Instant;

use crate::SearchProgress;

/// Clock, cancellation, and completed-iteration notification boundary.
///
/// Browser callers normally use `performance.now()` and a generation or atomic
/// cancellation flag. Native tools can use [`SystemControl`]. Implementations
/// should make these methods cheap: search calls them at batched node intervals.
pub trait SearchControl {
    fn now_ms(&self) -> f64;

    fn should_stop(&self) -> bool {
        false
    }

    fn on_iteration(&self, _progress: &SearchProgress) {}
}

/// A deterministic control for fixed-node searches. Time never advances.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoopControl;

impl SearchControl for NoopControl {
    fn now_ms(&self) -> f64 {
        0.0
    }
}

/// Monotonic native clock suitable for CLI use and tests.
#[derive(Debug)]
pub struct SystemControl {
    started: Instant,
}

impl SystemControl {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
        }
    }
}

impl Default for SystemControl {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchControl for SystemControl {
    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_clock_is_stable() {
        let clock = NoopControl;
        assert_eq!(clock.now_ms(), 0.0);
        assert!(!clock.should_stop());
    }

    #[test]
    fn system_clock_is_monotonic() {
        let clock = SystemControl::new();
        let first = clock.now_ms();
        let second = clock.now_ms();
        assert!(second >= first);
    }
}
