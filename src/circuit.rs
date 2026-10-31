//! Circuit breaker around Google. After repeated failures it fails fast for a
//! cooldown, then lets one trial call through to test recovery.

use std::sync::Mutex;
use std::time::{Duration, Instant};

const FAILURE_LIMIT: u32 = 5;
const COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Default)]
struct State {
    failures: u32,
    open_until: Option<Instant>,
}

#[derive(Default)]
pub struct Circuit {
    state: Mutex<State>,
}

impl Circuit {
    /// False while open. After the cooldown one caller gets through; the
    /// timer is pushed forward so the others keep failing fast.
    pub fn allow(&self) -> bool {
        self.allow_at(Instant::now())
    }
    pub fn success(&self) {
        *self.state.lock().unwrap() = State::default();
    }
    pub fn failure(&self) {
        self.failure_at(Instant::now());
    }
    pub fn is_open(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.open_until.is_some_and(|t| Instant::now() < t)
    }

    fn allow_at(&self, now: Instant) -> bool {
        let mut state = self.state.lock().unwrap();
        match state.open_until {
            Some(until) if now < until => false,
            Some(_) => {
                state.open_until = Some(now + COOLDOWN);
                true
            }
            None => true,
        }
    }
    fn failure_at(&self, now: Instant) {
        let mut state = self.state.lock().unwrap();
        state.failures += 1;
        if state.failures >= FAILURE_LIMIT {
            state.open_until = Some(now + COOLDOWN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_after_limit_then_trials_once_and_recovers() {
        let c = Circuit::default();
        let t = Instant::now();
        for _ in 0..FAILURE_LIMIT - 1 {
            c.failure_at(t);
        }
        assert!(c.allow_at(t));
        c.failure_at(t);
        assert!(!c.allow_at(t));
        let later = t + COOLDOWN;
        assert!(c.allow_at(later));
        assert!(!c.allow_at(later));
        c.success();
        assert!(c.allow_at(later));
    }

    #[test]
    fn success_resets_the_failure_count() {
        let c = Circuit::default();
        let t = Instant::now();
        for _ in 0..FAILURE_LIMIT - 1 {
            c.failure_at(t);
        }
        c.success();
        c.failure_at(t);
        assert!(c.allow_at(t));
    }
}
