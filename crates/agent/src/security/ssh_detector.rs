//! Sliding-window SSH brute-force detector.
//!
//! For each source IP we keep a `VecDeque<(Instant, AuthAttempt)>` clipped to
//! `window`. When the number of failed attempts in the window reaches
//! `threshold` the IP is flagged with a severity escalated by the number of
//! *distinct usernames* observed:
//!
//! * 1 distinct user        → `Severity::Medium`
//! * 2 – 4 distinct users   → `Severity::High`
//! * ≥ 5 distinct users     → `Severity::Critical`
//!
//! After the detector fires for an IP its queue is cleared so we don't
//! re-emit on every additional failed attempt (a fresh window must build up
//! again before the next event).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use serverbee_common::security::{SecurityEvidence, Severity, SshAuthMethod};

use crate::security::ssh_parser::{AuthAttempt, AuthMethodHint, AuthOutcome};

type Clock = Box<dyn Fn() -> Instant + Send + Sync>;

pub struct SshDetector {
    window: Duration,
    threshold: u32,
    clock: Clock,
    per_ip: HashMap<String, VecDeque<(Instant, AuthAttempt)>>,
}

#[derive(Debug)]
pub enum DetectorEmit {
    None,
    BruteForce {
        source_ip: String,
        severity: Severity,
        evidence: SecurityEvidence,
        started_at: Instant,
        ended_at: Instant,
    },
    Login {
        username: String,
        source_ip: String,
        source_port: Option<u16>,
        auth_method: SshAuthMethod,
    },
}

impl SshDetector {
    pub fn new(window: Duration, threshold: u32) -> Self {
        Self {
            window,
            threshold,
            clock: Box::new(Instant::now),
            per_ip: HashMap::new(),
        }
    }

    pub fn with_clock(
        window: Duration,
        threshold: u32,
        clock: impl Fn() -> Instant + Send + Sync + 'static,
    ) -> Self {
        Self {
            window,
            threshold,
            clock: Box::new(clock),
            per_ip: HashMap::new(),
        }
    }

    /// Evict IPs whose deques are empty or contain only entries older than the
    /// window. Prevents unbounded growth under "spray" attacks where each
    /// source IP attempts once and never returns.
    pub fn sweep(&mut self) {
        let now = (self.clock)();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        self.per_ip.retain(|_, q| {
            while let Some((ts, _)) = q.front() {
                if *ts < cutoff {
                    q.pop_front();
                } else {
                    break;
                }
            }
            !q.is_empty()
        });
    }

    pub fn observe(&mut self, attempt: AuthAttempt) -> DetectorEmit {
        match attempt.outcome {
            AuthOutcome::Success { auth_method } => DetectorEmit::Login {
                username: attempt.username,
                source_ip: attempt.source_ip,
                source_port: attempt.source_port,
                auth_method: auth_method_to_common(auth_method),
            },
            AuthOutcome::Failure { .. } => self.observe_failure(attempt),
        }
    }

    fn observe_failure(&mut self, attempt: AuthAttempt) -> DetectorEmit {
        let now = (self.clock)();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        let ip = attempt.source_ip.clone();
        let entry = self.per_ip.entry(ip.clone()).or_default();
        // Expire old entries.
        while let Some((ts, _)) = entry.front() {
            if *ts < cutoff {
                entry.pop_front();
            } else {
                break;
            }
        }
        entry.push_back((now, attempt));

        if entry.len() as u32 >= self.threshold {
            let attempts: Vec<&AuthAttempt> = entry.iter().map(|(_, a)| a).collect();
            let failed_count = attempts.len() as u32;
            let mut users: HashSet<&str> = HashSet::new();
            let mut invalid_user_count: u32 = 0;
            for a in &attempts {
                users.insert(a.username.as_str());
                if matches!(a.outcome, AuthOutcome::Failure { invalid_user: true }) {
                    invalid_user_count += 1;
                }
            }
            let distinct_users = users.len() as u32;
            let severity = match distinct_users {
                0 | 1 => Severity::Medium,
                2..=4 => Severity::High,
                _ => Severity::Critical,
            };
            // sample_users: up to first 5 distinct usernames in insertion order.
            let mut sample_users: Vec<String> = Vec::new();
            let mut seen: HashSet<&str> = HashSet::new();
            for a in &attempts {
                if seen.insert(a.username.as_str()) {
                    sample_users.push(a.username.clone());
                    if sample_users.len() >= 5 {
                        break;
                    }
                }
            }
            let started_at = entry.front().map(|(t, _)| *t).unwrap_or(now);
            let ended_at = now;
            let window_seconds = self.window.as_secs() as u32;
            let threshold = self.threshold;
            entry.clear();

            DetectorEmit::BruteForce {
                source_ip: ip,
                severity,
                evidence: SecurityEvidence::SshBruteForce {
                    failed_count,
                    distinct_users,
                    sample_users,
                    invalid_user_count,
                    window_seconds,
                    threshold,
                },
                started_at,
                ended_at,
            }
        } else {
            DetectorEmit::None
        }
    }
}

fn auth_method_to_common(hint: AuthMethodHint) -> SshAuthMethod {
    match hint {
        AuthMethodHint::Publickey => SshAuthMethod::Publickey,
        AuthMethodHint::Password => SshAuthMethod::Password,
        AuthMethodHint::KeyboardInteractive => SshAuthMethod::KeyboardInteractive,
        AuthMethodHint::Other => SshAuthMethod::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn attempt(user: &str, ip: &str, success: bool) -> AuthAttempt {
        AuthAttempt {
            outcome: if success {
                AuthOutcome::Success {
                    auth_method: AuthMethodHint::Publickey,
                }
            } else {
                AuthOutcome::Failure {
                    invalid_user: false,
                }
            },
            username: user.into(),
            source_ip: ip.into(),
            source_port: Some(22),
        }
    }

    #[test]
    fn single_user_hammering_triggers_medium() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = SshDetector::with_clock(Duration::from_secs(60), 10, move || {
            *nowc.lock().unwrap()
        });
        for _ in 0..9 {
            assert!(matches!(
                det.observe(attempt("root", "1.2.3.4", false)),
                DetectorEmit::None
            ));
        }
        let emit = det.observe(attempt("root", "1.2.3.4", false));
        match emit {
            DetectorEmit::BruteForce {
                severity,
                source_ip,
                evidence,
                ..
            } => {
                assert_eq!(severity, Severity::Medium);
                assert_eq!(source_ip, "1.2.3.4");
                if let SecurityEvidence::SshBruteForce {
                    failed_count,
                    distinct_users,
                    threshold,
                    ..
                } = evidence
                {
                    assert_eq!(failed_count, 10);
                    assert_eq!(distinct_users, 1);
                    assert_eq!(threshold, 10);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected brute force trigger"),
        }
    }

    #[test]
    fn two_to_four_users_escalates_to_high() {
        let mut det = SshDetector::new(Duration::from_secs(60), 4);
        det.observe(attempt("root", "1.2.3.4", false));
        det.observe(attempt("admin", "1.2.3.4", false));
        det.observe(attempt("ubuntu", "1.2.3.4", false));
        let last = det.observe(attempt("postgres", "1.2.3.4", false));
        match last {
            DetectorEmit::BruteForce { severity, .. } => assert_eq!(severity, Severity::High),
            _ => panic!("expected fire"),
        }
    }

    #[test]
    fn five_or_more_users_escalates_to_critical() {
        let mut det = SshDetector::new(Duration::from_secs(60), 5);
        for u in &["root", "admin", "ubuntu", "postgres", "git"] {
            det.observe(attempt(u, "1.2.3.4", false));
        }
        // The 5th attempt hits the threshold and 5 distinct users → Critical.
        // Re-create detector and try with 6 distinct users to be thorough.
        let mut det = SshDetector::new(Duration::from_secs(60), 6);
        for u in &["root", "admin", "ubuntu", "postgres", "git"] {
            det.observe(attempt(u, "1.2.3.4", false));
        }
        let last = det.observe(attempt("nginx", "1.2.3.4", false));
        match last {
            DetectorEmit::BruteForce { severity, .. } => assert_eq!(severity, Severity::Critical),
            _ => panic!("expected fire"),
        }
    }

    #[test]
    fn window_expiry_resets() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = SshDetector::with_clock(Duration::from_secs(60), 3, move || {
            *nowc.lock().unwrap()
        });
        det.observe(attempt("root", "1.2.3.4", false));
        det.observe(attempt("root", "1.2.3.4", false));
        *now.lock().unwrap() += Duration::from_secs(120);
        // First two should now have aged out.
        assert!(matches!(
            det.observe(attempt("root", "1.2.3.4", false)),
            DetectorEmit::None
        ));
    }

    #[test]
    fn success_emits_login() {
        let mut det = SshDetector::new(Duration::from_secs(60), 10);
        let e = det.observe(attempt("ubuntu", "5.6.7.8", true));
        match e {
            DetectorEmit::Login {
                username,
                source_ip,
                auth_method,
                ..
            } => {
                assert_eq!(username, "ubuntu");
                assert_eq!(source_ip, "5.6.7.8");
                assert!(matches!(auth_method, SshAuthMethod::Publickey));
            }
            _ => panic!("expected login emit"),
        }
    }

    #[test]
    fn fire_clears_queue_so_next_window_must_refill() {
        let mut det = SshDetector::new(Duration::from_secs(60), 3);
        for _ in 0..3 {
            det.observe(attempt("root", "1.2.3.4", false));
        }
        // Next attempt should be back to None — queue cleared after fire.
        assert!(matches!(
            det.observe(attempt("root", "1.2.3.4", false)),
            DetectorEmit::None
        ));
    }

    #[test]
    fn invalid_user_counted_in_evidence() {
        let mut det = SshDetector::new(Duration::from_secs(60), 3);
        let inv = AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user: true },
            username: "fake".into(),
            source_ip: "1.2.3.4".into(),
            source_port: Some(22),
        };
        det.observe(inv.clone());
        det.observe(inv.clone());
        let last = det.observe(inv);
        match last {
            DetectorEmit::BruteForce { evidence, .. } => {
                if let SecurityEvidence::SshBruteForce {
                    invalid_user_count, ..
                } = evidence
                {
                    assert_eq!(invalid_user_count, 3);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected fire"),
        }
    }
}
