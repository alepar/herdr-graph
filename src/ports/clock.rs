use crate::model::Timestamp;
use std::sync::Mutex;

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now()
    }
}

/// Fake clock for tests.
pub struct ManualClock(Mutex<Timestamp>);
impl ManualClock {
    pub fn new(t: Timestamp) -> Self {
        Self(Mutex::new(t))
    }
    pub fn set(&self, t: Timestamp) {
        *self.0.lock().unwrap() = t;
    }
    pub fn advance(&self, d: chrono::Duration) {
        *self.0.lock().unwrap() += d;
    }
}
impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        *self.0.lock().unwrap()
    }
}
