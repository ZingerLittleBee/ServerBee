//! One time boundary for renewal lifecycle and expiration reminder evaluation.
use chrono::{DateTime, Utc};

pub trait RenewalClock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemRenewalClock;
impl RenewalClock for SystemRenewalClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
