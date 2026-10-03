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
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
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
    journal: Mutex<super::journal::Journal>,
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
    pub fn try_new(base: u32, grants_path: PathBuf) -> anyhow::Result<Arc<Self>> {
        let store = CapabilityGrantStore::load(&grants_path);
        let effective = (base | store.active_bits(now_unix(), base)) & CAP_VALID_MASK;
        let (state_tx, _) = watch::channel(effective);
        let (transition_tx, _) = broadcast::channel(16);
        let journal =
            super::journal::Journal::open(&grants_path, store.active_bits(now_unix(), base))?;
        Ok(Arc::new(Self {
            journal: Mutex::new(journal),
            base,
            grants_path,
            effective: AtomicU32::new(effective),
            state_tx,
            transition_tx,
        }))
    }

    #[cfg(test)]
    pub fn new(base: u32, grants_path: PathBuf) -> Arc<Self> {
        Self::try_new(base, grants_path).expect("test capability journal")
    }

    /// Bind retained events to the enrolled Server and deployment. A different
    /// owner cannot receive the prior owner's events. Ordinary token rotation
    /// for the same enrolled Server preserves them.
    pub fn bind_destination(&self, destination: &str) -> anyhow::Result<()> {
        let mut journal = self
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("capability journal lock poisoned"))?;
        let mut next = journal.clone();
        if next
            .destination
            .as_deref()
            .is_some_and(|old| old != destination)
        {
            next.events.clear();
        }
        next.destination = Some(destination.to_string());
        next.flush()?;
        *journal = next;
        Ok(())
    }

    pub fn pending_events(&self) -> anyhow::Result<Vec<super::journal::Event>> {
        let journal = self
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("capability journal lock poisoned"))?;
        if journal.dirty {
            anyhow::bail!("capability source is not durably saved yet");
        }
        Ok(journal.events.clone())
    }

    pub fn acknowledge_event(&self, msg_id: &str) -> anyhow::Result<()> {
        let mut journal = self
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("capability journal lock poisoned"))?;
        if !journal.events.iter().any(|event| event.msg_id == msg_id) {
            return Ok(());
        }
        let mut next = journal.clone();
        next.events.retain(|event| event.msg_id != msg_id);
        next.flush()?;
        *journal = next;
        Ok(())
    }

    /// A legacy peer cannot confirm admission. Preserve its historical
    /// fire-and-forget behavior rather than replaying already attempted events
    /// as new notifications after a future peer upgrade.
    pub fn discard_legacy_events(&self) -> anyhow::Result<()> {
        let mut journal = self
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("capability journal lock poisoned"))?;
        if journal.events.is_empty() {
            return Ok(());
        }
        let mut next = journal.clone();
        next.events.clear();
        next.flush()?;
        *journal = next;
        Ok(())
    }

    /// Apply local authority immediately, including revocations. Advance the
    /// durable observation cursor only after the original transition is saved.
    fn observe(&self, now: i64) -> anyhow::Result<Option<CapabilityTransition>> {
        let store = CapabilityGrantStore::load(&self.grants_path);
        let mut journal = self
            .journal
            .lock()
            .map_err(|_| anyhow::anyhow!("capability journal lock poisoned"))?;
        let (effective, active, temporary, changes) =
            evaluate(&store, self.base, journal.observed_active, now);
        let local_changed = effective != self.effective();
        if local_changed {
            self.effective.store(effective, Ordering::SeqCst);
            let _ = self.state_tx.send(effective);
        }
        if journal.dirty {
            if let Err(error) = journal.flush() {
                if local_changed {
                    let _ = self.transition_tx.send(CapabilityTransition {
                        effective,
                        temporary: temporary.clone(),
                        changes: Vec::new(),
                    });
                }
                return Err(error.into());
            }
            journal.dirty = false;
        }
        if active == journal.observed_active {
            return Ok(None);
        }
        let mut next = journal.clone();
        next.observed_active = active;
        for change in &changes {
            // The CLI grant record is the original source. Restart before the
            // first report must not restart its delivery clock at observation.
            let occurred_at = if matches!(change.action, CapabilityChangeAction::Granted) {
                store
                    .records()
                    .find(|record| record.cap == change.cap)
                    .map_or(now, |record| record.granted_at)
            } else {
                now
            };
            next.events.push(super::journal::Event {
                msg_id: uuid::Uuid::new_v4().to_string(),
                occurred_at: chrono::DateTime::from_timestamp(occurred_at, 0)
                    .ok_or_else(|| anyhow::anyhow!("invalid capability source time"))?,
                changes: vec![change.clone()],
            });
        }
        next.dirty = true;
        *journal = next;
        if let Err(error) = journal.flush() {
            if local_changed {
                let _ = self.transition_tx.send(CapabilityTransition {
                    effective,
                    temporary: temporary.clone(),
                    changes: Vec::new(),
                });
            }
            return Err(error.into());
        }
        journal.dirty = false;
        Ok(Some(CapabilityTransition {
            effective,
            temporary,
            changes,
        }))
    }

    /// Fixed-state authority whose effective caps equal `base` and never
    /// change (no grants file, no running loop). Test-only: production code
    /// always gates on the process-wide authority built in `main`.
    #[cfg(test)]
    pub fn fixed(base: u32) -> Arc<Self> {
        Self::new(
            base,
            std::env::temp_dir()
                .join(format!("sb-fixed-{}", uuid::Uuid::new_v4()))
                .join("grants.json"),
        )
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
        let mut interval = tokio::time::interval(tick);
        interval.tick().await;
        loop {
            interval.tick().await;
            match self.observe(now_unix()) {
                Ok(Some(transition)) => {
                    let _ = self.transition_tx.send(transition);
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::error!(error = %error, "Capability source observation remains unconsumed")
                }
            }
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

#[cfg(test)]
mod journal_tests {
    use super::*;
    use crate::capability_grants::store::GrantRecord;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    #[test]
    fn original_transition_survives_restart_until_ack_and_destination_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grants.json");
        let authority = CapabilityAuthority::new(CAP_DEFAULT, path.clone());
        authority.bind_destination("deployment:server-a").unwrap();
        let mut store = CapabilityGrantStore::load(&path);
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: 1000,
                expires_at: 2000,
                granted_by: "root".into(),
                reason: None,
            },
            1000,
        );
        store.flush().unwrap();
        authority.observe(1001).unwrap();
        let original = serde_json::to_value(authority.pending_events().unwrap()).unwrap();
        assert_eq!(original.as_array().unwrap().len(), 1);
        assert_eq!(
            authority.pending_events().unwrap()[0]
                .occurred_at
                .timestamp(),
            1000,
            "A grant uses its original CLI source time, not the later observation time"
        );
        drop(authority);
        let restarted = CapabilityAuthority::new(CAP_DEFAULT, path.clone());
        restarted.bind_destination("deployment:server-a").unwrap();
        assert_eq!(
            serde_json::to_value(restarted.pending_events().unwrap()).unwrap(),
            original
        );
        assert!(
            restarted.observe(1002).unwrap().is_none(),
            "Snapshot never manufactures another grant"
        );
        store.remove("terminal", 1003);
        store.flush().unwrap();
        restarted.observe(1003).unwrap();
        assert_eq!(
            restarted.effective() & CAP_TERMINAL,
            0,
            "Original grant replay cannot change live authority"
        );
        let pending = restarted.pending_events().unwrap();
        assert_eq!(pending.len(), 2);
        assert!(matches!(
            pending[1].changes[0].action,
            CapabilityChangeAction::Revoked
        ));
        restarted.acknowledge_event(&pending[0].msg_id).unwrap();
        restarted.acknowledge_event("unknown-frame").unwrap();
        drop(restarted);
        let final_restart = CapabilityAuthority::new(CAP_DEFAULT, path);
        assert_eq!(final_restart.pending_events().unwrap().len(), 1);
        final_restart
            .bind_destination("deployment:server-b")
            .unwrap();
        assert!(
            final_restart.pending_events().unwrap().is_empty(),
            "Another enrollment cannot receive prior events"
        );
    }

    #[test]
    fn source_write_failure_retains_original_transition_and_revocation_still_applies() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grants.json");
        let authority = CapabilityAuthority::new(CAP_DEFAULT, path.clone());
        let mut store = CapabilityGrantStore::load(&path);
        store.upsert(
            GrantRecord {
                cap: "terminal".into(),
                granted_at: 1000,
                expires_at: 2000,
                granted_by: "root".into(),
                reason: None,
            },
            1000,
        );
        store.flush().unwrap();
        // The journal's atomic rename fails against a directory, after the
        // authority has observed the real grants file but before source consumption.
        std::fs::remove_file(path.with_extension("events.json")).unwrap();
        std::fs::create_dir(path.with_extension("events.json")).unwrap();
        assert!(authority.observe(1001).is_err());
        assert!(authority.pending_events().is_err());
        let pending = authority.journal.lock().unwrap().events[0].clone();
        let mut transitions = authority.subscribe_transitions();
        store.remove("terminal", 1002);
        store.flush().unwrap();
        assert!(authority.observe(1002).is_err());
        assert_eq!(authority.effective() & CAP_TERMINAL, 0);
        assert_eq!(
            transitions.try_recv().unwrap().effective & CAP_TERMINAL,
            0,
            "Revocation tears down local sessions even while source storage is unavailable"
        );
        std::fs::remove_dir(path.with_extension("events.json")).unwrap();
        authority.observe(1005).unwrap();
        let recovered = authority.pending_events().unwrap();
        assert_eq!(recovered[0].msg_id, pending.msg_id);
        assert_eq!(recovered[0].occurred_at, pending.occurred_at);
        store.remove("terminal", 1006);
        store.flush().unwrap();
        authority.observe(1006).unwrap();
        assert_eq!(authority.effective() & CAP_TERMINAL, 0);
    }
}
