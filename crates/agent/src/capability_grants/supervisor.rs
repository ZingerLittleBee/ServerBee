use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use serverbee_common::constants::{ALL_CAPABILITIES, CAP_VALID_MASK};
use serverbee_common::protocol::{
    AgentMessage, CapabilityChangeAction, CapabilityChangeEvent, TemporaryGrant,
};

use super::store::CapabilityGrantStore;

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
    let temporary = store.active_grants(now);

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

/// Long-running per-connection task: re-reads the grants file, updates the
/// shared effective-caps cache, and emits `CapabilitiesChanged` on transitions.
/// Read-only on the file (the CLI is the only writer). Stops when `tx` closes
/// (i.e. the connection ended).
pub async fn run_grant_supervisor(
    grants_path: PathBuf,
    base: u32,
    capabilities: Arc<AtomicU32>,
    tx: mpsc::Sender<AgentMessage>,
    tick: Duration,
) {
    // Seed prev_active from the current file so grants already active at connect
    // time are NOT re-announced as new (avoids alert spam on every reconnect).
    let mut prev_active = CapabilityGrantStore::load(&grants_path).active_bits(now_unix(), base);
    let mut interval = tokio::time::interval(tick);
    interval.tick().await; // consume the immediate first tick

    loop {
        interval.tick().await;
        let now = now_unix();
        let store = CapabilityGrantStore::load(&grants_path);
        let (effective, active_bits, temporary, changes) =
            evaluate(&store, base, prev_active, now);

        if effective != capabilities.load(Ordering::SeqCst) {
            capabilities.store(effective, Ordering::SeqCst);
            let msg = AgentMessage::CapabilitiesChanged {
                msg_id: uuid::Uuid::new_v4().to_string(),
                capabilities: effective,
                temporary,
                changes,
            };
            if tx.send(msg).await.is_err() {
                tracing::debug!("grant supervisor channel closed; stopping");
                break;
            }
            tracing::info!(effective, "capability grant state changed");
        }
        prev_active = active_bits;
    }
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
}
