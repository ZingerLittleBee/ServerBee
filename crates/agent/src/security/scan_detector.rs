//! Port-scan detector.
//!
//! For each source IP we keep a `VecDeque<(Instant, port)>` of recent
//! connection attempts plus a `HashMap<port, count>` so we can answer
//! "distinct ports in the current window" in `O(1)`. When the distinct port
//! count reaches `threshold` we emit a `PortScan` event and clear that IP's
//! state to debounce subsequent emits.
//!
//! `record_blocked` is fed by the kernel firewall log stream so we can
//! enrich the emitted evidence with `blocked_count`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use serverbee_common::security::SecurityEvidence;

type Clock = Box<dyn Fn() -> Instant + Send + Sync>;

pub struct ScanDetector {
    window: Duration,
    threshold: u32,
    clock: Clock,
    per_ip: HashMap<String, ScanState>,
}

struct ScanState {
    events: VecDeque<(Instant, u16)>,
    port_counts: HashMap<u16, u32>,
    total: u32,
    blocked: u32,
    window_started_at: Option<Instant>,
}

impl ScanState {
    fn new(now: Instant) -> Self {
        Self {
            events: VecDeque::new(),
            port_counts: HashMap::new(),
            total: 0,
            blocked: 0,
            window_started_at: Some(now),
        }
    }
}

#[derive(Debug)]
pub enum ScanEmit {
    None,
    PortScan {
        source_ip: String,
        evidence: SecurityEvidence,
        started_at: Instant,
        ended_at: Instant,
    },
}

impl ScanDetector {
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

    /// Record an inbound connection attempt and possibly emit a PortScan event.
    pub fn observe(&mut self, source_ip: String, dst_port: u16) -> ScanEmit {
        let now = (self.clock)();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        let state = self
            .per_ip
            .entry(source_ip.clone())
            .or_insert_with(|| ScanState::new(now));
        Self::expire_head(state, cutoff);
        state.events.push_back((now, dst_port));
        *state.port_counts.entry(dst_port).or_insert(0) += 1;
        state.total += 1;
        if state.window_started_at.is_none() {
            state.window_started_at = Some(now);
        }

        if state.port_counts.len() as u32 >= self.threshold {
            // Build evidence sample (up to 20 distinct ports).
            let mut sample_ports: Vec<u16> = Vec::with_capacity(20);
            let mut seen = std::collections::HashSet::new();
            for (_, p) in &state.events {
                if seen.insert(*p) {
                    sample_ports.push(*p);
                    if sample_ports.len() >= 20 {
                        break;
                    }
                }
            }
            let evidence = SecurityEvidence::PortScan {
                distinct_ports: state.port_counts.len() as u32,
                sample_ports,
                total_attempts: state.total,
                window_seconds: self.window.as_secs() as u32,
                threshold: self.threshold,
                blocked_count: state.blocked,
            };
            let started_at = state.window_started_at.unwrap_or(now);
            let ended_at = now;
            // Reset state to debounce.
            state.events.clear();
            state.port_counts.clear();
            state.total = 0;
            state.blocked = 0;
            state.window_started_at = None;
            ScanEmit::PortScan {
                source_ip,
                evidence,
                started_at,
                ended_at,
            }
        } else {
            ScanEmit::None
        }
    }

    /// Increment the blocked counter for an IP so it shows up in the next
    /// emitted PortScan evidence. Called from the firewall log stream.
    pub fn record_blocked(&mut self, source_ip: &str) {
        let now = (self.clock)();
        let state = self
            .per_ip
            .entry(source_ip.to_string())
            .or_insert_with(|| ScanState::new(now));
        state.blocked = state.blocked.saturating_add(1);
    }

    /// Walk every IP and expire stale window entries. Cheap; can be called
    /// periodically by the manager.
    pub fn sweep(&mut self) {
        let now = (self.clock)();
        let cutoff = now.checked_sub(self.window).unwrap_or(now);
        for state in self.per_ip.values_mut() {
            Self::expire_head(state, cutoff);
        }
        self.per_ip
            .retain(|_, s| !s.events.is_empty() || s.blocked > 0);
    }

    fn expire_head(state: &mut ScanState, cutoff: Instant) {
        while let Some((ts, port)) = state.events.front().copied() {
            if ts < cutoff {
                state.events.pop_front();
                if let Some(c) = state.port_counts.get_mut(&port) {
                    *c -= 1;
                    if *c == 0 {
                        state.port_counts.remove(&port);
                    }
                }
                state.total = state.total.saturating_sub(1);
            } else {
                break;
            }
        }
        if state.events.is_empty() {
            state.window_started_at = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn distinct_ports_threshold_triggers() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 20);
        for port in 1..20 {
            assert!(matches!(
                det.observe("1.2.3.4".into(), port),
                ScanEmit::None
            ));
        }
        let emit = det.observe("1.2.3.4".into(), 20);
        match emit {
            ScanEmit::PortScan { evidence, .. } => {
                if let SecurityEvidence::PortScan {
                    distinct_ports,
                    threshold,
                    total_attempts,
                    ..
                } = evidence
                {
                    assert_eq!(distinct_ports, 20);
                    assert_eq!(threshold, 20);
                    assert_eq!(total_attempts, 20);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected scan to fire"),
        }
    }

    #[test]
    fn same_port_repeat_does_not_trigger() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 20);
        for _ in 0..50 {
            assert!(matches!(
                det.observe("1.2.3.4".into(), 22),
                ScanEmit::None
            ));
        }
    }

    #[test]
    fn window_slide_drops_ports() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = ScanDetector::with_clock(Duration::from_secs(30), 20, move || {
            *nowc.lock().unwrap()
        });
        for port in 1..15 {
            det.observe("1.2.3.4".into(), port);
        }
        *now.lock().unwrap() += Duration::from_secs(60);
        det.sweep();
        // All ports aged out — next attempt should not trigger.
        for port in 100..115 {
            assert!(matches!(
                det.observe("1.2.3.4".into(), port),
                ScanEmit::None
            ));
        }
    }

    #[test]
    fn firewall_blocked_count_threads_through_evidence() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 5);
        for _ in 0..3 {
            det.record_blocked("1.2.3.4");
        }
        for port in 1..5 {
            det.observe("1.2.3.4".into(), port);
        }
        let emit = det.observe("1.2.3.4".into(), 5);
        match emit {
            ScanEmit::PortScan { evidence, .. } => {
                if let SecurityEvidence::PortScan { blocked_count, .. } = evidence {
                    assert_eq!(blocked_count, 3);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected scan to fire"),
        }
    }

    #[test]
    fn fire_clears_state_so_next_window_must_refill() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 5);
        for port in 1..=5 {
            det.observe("1.2.3.4".into(), port);
        }
        // Threshold was hit on the 5th port; subsequent single ports should not refire.
        for port in 100..104 {
            assert!(matches!(
                det.observe("1.2.3.4".into(), port),
                ScanEmit::None
            ));
        }
    }

    #[test]
    fn sweep_evicts_empty_ip_but_keeps_blocked_only_ip() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = ScanDetector::with_clock(Duration::from_secs(30), 20, move || {
            *nowc.lock().unwrap()
        });
        // IP "a" gets a couple of port events; IP "b" only a blocked counter.
        det.observe("a".into(), 1);
        det.observe("a".into(), 2);
        det.record_blocked("b");
        assert_eq!(det.per_ip.len(), 2);
        // Age every event out of the window.
        *now.lock().unwrap() += Duration::from_secs(60);
        det.sweep();
        // "a" drained empty with blocked==0 → evicted; "b" kept (blocked>0).
        assert!(det.per_ip.contains_key("b"));
        assert!(!det.per_ip.contains_key("a"));
    }

    #[test]
    fn window_slide_decrements_port_counts() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = ScanDetector::with_clock(Duration::from_secs(30), 20, move || {
            *nowc.lock().unwrap()
        });
        // Ten distinct ports at t0.
        for port in 1..=10 {
            det.observe("1.2.3.4".into(), port);
        }
        // Advance past window so all ten age out via expire_head on next observe,
        // exercising the per-port count decrement + removal branch.
        *now.lock().unwrap() += Duration::from_secs(60);
        // One fresh port: expire_head drops the 10 old ports, leaving 1 distinct.
        assert!(matches!(
            det.observe("1.2.3.4".into(), 100),
            ScanEmit::None
        ));
        // Add 18 more distinct fresh ports → 19 distinct, still below 20.
        for port in 101..=118 {
            assert!(matches!(
                det.observe("1.2.3.4".into(), port),
                ScanEmit::None
            ));
        }
        // 20th distinct fresh port fires; old ports must NOT inflate the count.
        match det.observe("1.2.3.4".into(), 119) {
            ScanEmit::PortScan { evidence, .. } => {
                if let SecurityEvidence::PortScan { distinct_ports, .. } = evidence {
                    assert_eq!(distinct_ports, 20);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected fire"),
        }
    }

    #[test]
    fn repeated_port_count_decrements_without_removal() {
        // Same port observed twice keeps the entry after one expiry.
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = ScanDetector::with_clock(Duration::from_secs(30), 20, move || {
            *nowc.lock().unwrap()
        });
        det.observe("1.2.3.4".into(), 22); // t0, count(22)=1
        *now.lock().unwrap() += Duration::from_secs(10);
        det.observe("1.2.3.4".into(), 22); // t0+10, count(22)=2
        // Move time so only the first entry (t0) ages out.
        *now.lock().unwrap() += Duration::from_secs(25); // now = t0+35, cutoff = t0+5
        det.sweep();
        // Port 22 still present (count decremented 2 -> 1, not removed).
        assert_eq!(det.per_ip.get("1.2.3.4").unwrap().port_counts.get(&22), Some(&1));
        assert_eq!(det.per_ip.get("1.2.3.4").unwrap().total, 1);
    }

    #[test]
    fn sample_ports_capped_at_twenty() {
        let mut det = ScanDetector::new(Duration::from_secs(60), 25);
        for port in 1..=24 {
            det.observe("1.2.3.4".into(), port);
        }
        match det.observe("1.2.3.4".into(), 25) {
            ScanEmit::PortScan { evidence, .. } => {
                if let SecurityEvidence::PortScan {
                    sample_ports,
                    distinct_ports,
                    ..
                } = evidence
                {
                    assert_eq!(distinct_ports, 25);
                    assert_eq!(sample_ports.len(), 20);
                } else {
                    panic!("wrong evidence variant");
                }
            }
            _ => panic!("expected fire"),
        }
    }

    #[test]
    fn record_blocked_saturates_at_max() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 5);
        // Pre-seed the blocked counter to u32::MAX to exercise saturating_add.
        det.record_blocked("1.2.3.4");
        det.per_ip.get_mut("1.2.3.4").unwrap().blocked = u32::MAX;
        det.record_blocked("1.2.3.4");
        assert_eq!(det.per_ip.get("1.2.3.4").unwrap().blocked, u32::MAX);
    }

    #[test]
    fn record_blocked_creates_state_for_new_ip() {
        let mut det = ScanDetector::new(Duration::from_secs(30), 5);
        det.record_blocked("9.9.9.9");
        assert_eq!(det.per_ip.get("9.9.9.9").unwrap().blocked, 1);
    }

    #[test]
    fn sweep_resets_window_started_at_when_drained() {
        let now = Arc::new(Mutex::new(Instant::now()));
        let nowc = now.clone();
        let mut det = ScanDetector::with_clock(Duration::from_secs(30), 20, move || {
            *nowc.lock().unwrap()
        });
        det.record_blocked("1.2.3.4"); // keeps the IP alive after sweep
        det.observe("1.2.3.4".into(), 1);
        assert!(det.per_ip.get("1.2.3.4").unwrap().window_started_at.is_some());
        *now.lock().unwrap() += Duration::from_secs(60);
        det.sweep();
        // Events drained but IP retained via blocked>0; window_started_at reset.
        let st = det.per_ip.get("1.2.3.4").unwrap();
        assert!(st.events.is_empty());
        assert!(st.window_started_at.is_none());
    }
}
