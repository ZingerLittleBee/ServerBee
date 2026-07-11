//! Agent-owned authority over effective runtime capabilities.
//!
//! One process-wide [`CapabilityAuthority`] owns the base bitmask, folds in
//! temporary grants from the grants file, and drives every transition:
//! updating the effective state, notifying long-running subsystems so they can
//! reconcile in-flight work, and handing connections the change events they
//! forward to the server. Consumers ask [`CapabilityAuthority::has`] /
//! [`CapabilityAuthority::effective`] and never learn where the bits come
//! from or when they change.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::{broadcast, watch};

use serverbee_common::constants::{ALL_CAPABILITIES, CAP_VALID_MASK, has_capability};
use serverbee_common::protocol::{CapabilityChangeAction, CapabilityChangeEvent, TemporaryGrant};

use super::store::CapabilityGrantStore;

/// One transition of the effective capability state, as observed by the
/// authority's evaluation loop.
#[derive(Debug, Clone)]
pub struct CapabilityTransition {
    /// The new effective bitmask.
    pub effective: u32,
    /// Temporary grants active after the transition.
    pub temporary: Vec<TemporaryGrant>,
    /// Per-capability change events (granted / expired / revoked).
    pub changes: Vec<CapabilityChangeEvent>,
}

pub struct CapabilityAuthority {
    base: u32,
    grants_path: PathBuf,
    effective: AtomicU32,
    state_tx: watch::Sender<u32>,
    transition_tx: broadcast::Sender<CapabilityTransition>,
}

impl CapabilityAuthority {
    /// Build the authority, seeding the effective state from `base` plus any
    /// still-active grants in the grants file (so grants survive a restart).
    /// Call [`Self::run`] once to start the transition loop.
    pub fn new(base: u32, grants_path: PathBuf) -> Arc<Self> {
        let store = CapabilityGrantStore::load(&grants_path);
        let effective = (base | store.active_bits(now_unix(), base)) & CAP_VALID_MASK;
        let (state_tx, _) = watch::channel(effective);
        let (transition_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            base,
            grants_path,
            effective: AtomicU32::new(effective),
            state_tx,
            transition_tx,
        })
    }

    /// Fixed-state authority whose effective caps equal `base` and never
    /// change (no grants file, no running loop). Test-only: production code
    /// always gates on the process-wide authority built in `main`.
    #[cfg(test)]
    pub fn fixed(base: u32) -> Arc<Self> {
        Self::new(base, PathBuf::from("/nonexistent/capability_grants.json"))
    }

    /// Whether the capability bit is currently effective.
    pub fn has(&self, cap: u32) -> bool {
        has_capability(self.effective(), cap)
    }

    /// Consistent snapshot of the effective bitmask, for callers that gate
    /// several capabilities in one decision.
    pub fn effective(&self) -> u32 {
        self.effective.load(Ordering::SeqCst)
    }

    /// Currently-active temporary grants (fresh read of the grants file),
    /// for reporting in `SystemInfo`.
    pub fn active_grants(&self) -> Vec<TemporaryGrant> {
        CapabilityGrantStore::load(&self.grants_path).active_grants(now_unix(), self.base)
    }

    /// Watch the effective bitmask. Long-running subsystems use this to
    /// reconcile in-flight work when a capability appears or disappears.
    pub fn subscribe_state(&self) -> watch::Receiver<u32> {
        self.state_tx.subscribe()
    }

    /// Every transition with its change events. Connections forward these to
    /// the server as `CapabilitiesChanged`.
    pub fn subscribe_transitions(&self) -> broadcast::Receiver<CapabilityTransition> {
        self.transition_tx.subscribe()
    }

    /// Directly set the effective bits, bypassing the grants file. Test-only:
    /// lets gate tests exercise a capability flip without a running loop.
    #[cfg(test)]
    pub fn set_effective_for_test(&self, bits: u32) {
        self.effective.store(bits, Ordering::SeqCst);
        let _ = self.state_tx.send(bits);
    }

    /// Process-wide transition loop: re-reads the grants file every `tick`,
    /// updates the effective state, and fans out transitions. Read-only on
    /// the file (the CLI is the only writer). Runs for the agent's lifetime.
    pub async fn run(self: Arc<Self>, tick: Duration) {
        // Seed prev_active from the current file so grants already active at
        // startup are NOT re-announced as new (avoids alert spam).
        let mut prev_active =
            CapabilityGrantStore::load(&self.grants_path).active_bits(now_unix(), self.base);
        let mut interval = tokio::time::interval(tick);
        interval.tick().await; // consume the immediate first tick

        loop {
            interval.tick().await;

            let now = now_unix();
            let store = CapabilityGrantStore::load(&self.grants_path);
            let (effective, active_bits, temporary, changes) =
                evaluate(&store, self.base, prev_active, now);

            if effective != self.effective() {
                self.effective.store(effective, Ordering::SeqCst);
                let _ = self.state_tx.send(effective);
                let _ = self.transition_tx.send(CapabilityTransition {
                    effective,
                    temporary,
                    changes,
                });
                tracing::info!(effective, "capability grant state changed");
            }
            prev_active = active_bits;
        }
    }
}

/// Pure: given the previous active-grant bits and a freshly-loaded store,
/// compute new effective caps, new active bits, the active-grant DTOs, and the
/// change events to emit.
pub fn evaluate(
    store: &CapabilityGrantStore,
    base: u32,
    prev_active_bits: u32,
    now: i64,
) -> (u32, u32, Vec<TemporaryGrant>, Vec<CapabilityChangeEvent>) {
    let active_bits = store.active_bits(now, base);
    let effective = (base | active_bits) & CAP_VALID_MASK;
    let temporary = store.active_grants(now, base);

    let granted = active_bits & !prev_active_bits;
    let removed = prev_active_bits & !active_bits;
    let mut changes = Vec::new();

    for meta in ALL_CAPABILITIES {
        if granted & meta.bit != 0 {
            let rec = store.records().find(|r| r.cap == meta.key);
            changes.push(CapabilityChangeEvent {
                cap: meta.key.to_string(),
                action: CapabilityChangeAction::Granted,
                expires_at: rec.map(|r| r.expires_at),
                granted_by: rec.map(|r| r.granted_by.clone()),
                reason: rec.and_then(|r| r.reason.clone()),
            });
        }
        if removed & meta.bit != 0 {
            // A still-present record means time elapsed (expired); a gone
            // record means the operator revoked it.
            let rec = store.records().find(|r| r.cap == meta.key);
            changes.push(CapabilityChangeEvent {
                cap: meta.key.to_string(),
                action: if rec.is_some() {
                    CapabilityChangeAction::Expired
                } else {
                    CapabilityChangeAction::Revoked
                },
                expires_at: None,
                granted_by: None,
                reason: None,
            });
        }
    }
    (effective, active_bits, temporary, changes)
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_grants::store::GrantRecord;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    fn store_with(cap: &str, expires_at: i64) -> CapabilityGrantStore {
        let mut s = CapabilityGrantStore::default();
        s.upsert(
            GrantRecord {
                cap: cap.into(),
                granted_at: 0,
                expires_at,
                granted_by: "root".into(),
                reason: None,
            },
            0,
        );
        s
    }

    #[test]
    fn newly_active_emits_granted() {
        let store = store_with("terminal", 1000);
        let (eff, active, temp, changes) = evaluate(&store, CAP_DEFAULT, 0, 0);
        assert_eq!(eff, CAP_DEFAULT | CAP_TERMINAL);
        assert_eq!(active, CAP_TERMINAL);
        assert_eq!(temp.len(), 1);
        assert_eq!(temp[0].cap, "terminal");
        assert_eq!(temp[0].expires_at, 1000);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].action, CapabilityChangeAction::Granted);
        assert_eq!(changes[0].cap, "terminal");
    }

    #[test]
    fn no_change_when_prev_equals_active() {
        let store = store_with("terminal", 1000);
        let (_eff, _active, _temp, changes) = evaluate(&store, CAP_DEFAULT, CAP_TERMINAL, 0);
        assert!(changes.is_empty());
    }

    #[test]
    fn expiry_emits_expired_revoke_emits_revoked() {
        let store = store_with("terminal", 100);
        let (_e, active, _t, changes) = evaluate(&store, CAP_DEFAULT, CAP_TERMINAL, 200);
        assert_eq!(active, 0);
        assert_eq!(changes[0].action, CapabilityChangeAction::Expired);

        let empty = CapabilityGrantStore::default();
        let (_e, _a, _t, changes) = evaluate(&empty, CAP_DEFAULT, CAP_TERMINAL, 50);
        assert_eq!(changes[0].action, CapabilityChangeAction::Revoked);
    }

    #[test]
    fn granted_event_carries_grant_metadata() {
        // Exercise the `rec.map(...)`/`and_then(...)` arms of the Granted event
        // by giving the record a granted_by and a reason.
        let mut store = CapabilityGrantStore::default();
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: 0,
                expires_at: 1000,
                granted_by: "alice".into(),
                reason: Some("incident-42".into()),
            },
            0,
        );
        let (_eff, _active, _temp, changes) = evaluate(&store, CAP_DEFAULT, 0, 0);
        assert_eq!(changes.len(), 1);
        let ev = &changes[0];
        assert_eq!(ev.action, CapabilityChangeAction::Granted);
        assert_eq!(ev.expires_at, Some(1000));
        assert_eq!(ev.granted_by.as_deref(), Some("alice"));
        assert_eq!(ev.reason.as_deref(), Some("incident-42"));
    }

    #[test]
    fn evaluate_no_grants_yields_base_effective() {
        let store = CapabilityGrantStore::default();
        let (eff, active, temp, changes) = evaluate(&store, CAP_DEFAULT, 0, 0);
        assert_eq!(eff, CAP_DEFAULT);
        assert_eq!(active, 0);
        assert!(temp.is_empty());
        assert!(changes.is_empty());
    }

    #[test]
    fn fixed_authority_reflects_base_only() {
        let auth = CapabilityAuthority::fixed(CAP_DEFAULT);
        assert_eq!(auth.effective(), CAP_DEFAULT);
        assert!(!auth.has(CAP_TERMINAL));
        assert!(auth.active_grants().is_empty());
    }

    #[test]
    fn new_seeds_effective_from_persisted_grants() {
        // A still-active grant in the file is folded into the effective caps
        // at construction time, so grants survive an agent restart.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("capability_grants.json");
        let mut store = CapabilityGrantStore::load(&path);
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: 0,
                expires_at: i64::MAX,
                granted_by: "root".into(),
                reason: None,
            },
            0,
        );
        store.flush().unwrap();

        let auth = CapabilityAuthority::new(CAP_DEFAULT, path);
        assert!(auth.has(CAP_TERMINAL));
        assert_eq!(auth.active_grants().len(), 1);
    }

    #[tokio::test]
    async fn run_fans_out_transition_on_new_grant() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("capability_grants.json");
        let store = CapabilityGrantStore::load(&path);
        store.flush().unwrap();

        let auth = CapabilityAuthority::new(CAP_DEFAULT, path.clone());
        let mut transitions = auth.subscribe_transitions();
        let mut state = auth.subscribe_state();
        tokio::spawn(Arc::clone(&auth).run(Duration::from_millis(10)));

        // Let the loop seed `prev_active` from the still-empty file before the
        // grant lands, so the transition is observed rather than folded into
        // the seed (avoids a race in the Granted event assertion).
        tokio::time::sleep(Duration::from_millis(80)).await;

        let mut store = CapabilityGrantStore::load(&path);
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: 0,
                expires_at: i64::MAX,
                granted_by: "root".into(),
                reason: Some("debug".into()),
            },
            0,
        );
        store.flush().unwrap();

        let transition = tokio::time::timeout(Duration::from_secs(3), transitions.recv())
            .await
            .expect("transition should arrive within timeout")
            .expect("broadcast should deliver");
        assert_eq!(transition.effective, CAP_DEFAULT | CAP_TERMINAL);
        assert_eq!(transition.temporary.len(), 1);
        assert_eq!(transition.changes.len(), 1);
        assert_eq!(transition.changes[0].action, CapabilityChangeAction::Granted);

        // The state watch and the shared snapshot moved with the transition.
        tokio::time::timeout(Duration::from_secs(1), state.changed())
            .await
            .expect("state watch should flip")
            .expect("watch sender must be alive");
        assert_eq!(*state.borrow(), CAP_DEFAULT | CAP_TERMINAL);
        assert!(auth.has(CAP_TERMINAL));
    }
}
