//! Top-level firewall state machine: holds the desired blocklist mirror,
//! routes `ServerMessage::Blocklist*` to `nft`, and emits acks back via
//! the reporter.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use serverbee_common::firewall::{BlockEntry, BlocklistAckItem, BlocklistEntryState};
use serverbee_common::protocol::{AgentMessage, ServerMessage};
use tokio::sync::Mutex;

use crate::firewall::guardrail;
use crate::firewall::nft::{self, NftExecutor};

pub struct FirewallManager {
    /// Entries the agent has confirmed are in the kernel nft set.
    desired: Mutex<HashMap<String, BlockEntry>>,
    /// Resource-bootstrap state.
    nft_ready: Mutex<bool>,
    external_ip: Mutex<Option<IpAddr>>,
    executor: Arc<dyn NftExecutor>,
    /// Local capability — `false` means the `nft` probe failed at startup.
    #[allow(dead_code)]
    local_capable: bool,
}

impl FirewallManager {
    pub fn new(executor: Arc<dyn NftExecutor>, local_capable: bool) -> Self {
        Self {
            desired: Mutex::new(HashMap::new()),
            nft_ready: Mutex::new(false),
            external_ip: Mutex::new(None),
            executor,
            local_capable,
        }
    }

    pub async fn set_external_ip(&self, ip: Option<IpAddr>) {
        *self.external_ip.lock().await = ip;
    }

    /// Single entry point dispatched from the agent reporter loop.
    pub async fn handle(&self, msg: ServerMessage) -> Option<AgentMessage> {
        match msg {
            ServerMessage::BlocklistReset => Some(self.handle_reset().await),
            ServerMessage::BlocklistSync { entries } => Some(self.handle_sync(entries).await),
            ServerMessage::BlocklistAdd { entry } => Some(self.handle_add(entry).await),
            ServerMessage::BlocklistRemove { id } => Some(self.handle_remove(id).await),
            _ => None,
        }
    }

    async fn handle_reset(&self) -> AgentMessage {
        // Honored regardless of local capability — the kernel may still hold
        // stale rules from a previous capability=on window.
        match nft::unconditional_wipe(&*self.executor).await {
            Ok(()) => {
                self.desired.lock().await.clear();
                *self.nft_ready.lock().await = false;
                AgentMessage::BlocklistResetAck {
                    ok: true,
                    reason: None,
                }
            }
            Err(e) => AgentMessage::BlocklistResetAck {
                ok: false,
                reason: Some(e.to_string()),
            },
        }
    }

    async fn ensure_ready(&self) -> Result<(), String> {
        let mut g = self.nft_ready.lock().await;
        if *g {
            return Ok(());
        }
        nft::ensure_resources(&*self.executor)
            .await
            .map_err(|e| e.to_string())?;
        *g = true;
        Ok(())
    }

    async fn handle_sync(&self, entries: Vec<BlockEntry>) -> AgentMessage {
        if let Err(reason) = self.ensure_ready().await {
            // Whole pipeline broken — ack every entry as Failed.
            let results = entries
                .into_iter()
                .map(|e| BlocklistAckItem {
                    id: e.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(reason.clone()),
                })
                .collect();
            return AgentMessage::BlocklistAck { results };
        }

        let incoming: HashMap<String, BlockEntry> =
            entries.into_iter().map(|e| (e.id.clone(), e)).collect();

        let to_remove: Vec<BlockEntry> = {
            let g = self.desired.lock().await;
            g.values()
                .filter(|e| !incoming.contains_key(&e.id))
                .cloned()
                .collect()
        };

        let mut results = Vec::new();
        let own_ip = *self.external_ip.lock().await;

        for e in incoming.values() {
            if let Err(r) = guardrail::check(&e.target, own_ip) {
                self.desired.lock().await.remove(&e.id);
                results.push(BlocklistAckItem {
                    id: e.id.clone(),
                    state: BlocklistEntryState::Failed,
                    reason: Some(r),
                });
                continue;
            }
            match nft::add_element(&*self.executor, e).await {
                Ok(()) => {
                    self.desired.lock().await.insert(e.id.clone(), e.clone());
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Present,
                        reason: None,
                    });
                }
                Err(err) => {
                    self.desired.lock().await.remove(&e.id);
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Failed,
                        reason: Some(err.to_string()),
                    });
                }
            }
        }

        for e in &to_remove {
            match nft::delete_element(&*self.executor, e).await {
                Ok(()) => {
                    self.desired.lock().await.remove(&e.id);
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Absent,
                        reason: None,
                    });
                }
                Err(err) => {
                    // Kernel may still have it; keep desired so we retry later.
                    results.push(BlocklistAckItem {
                        id: e.id.clone(),
                        state: BlocklistEntryState::Failed,
                        reason: Some(err.to_string()),
                    });
                }
            }
        }

        AgentMessage::BlocklistAck { results }
    }

    async fn handle_add(&self, entry: BlockEntry) -> AgentMessage {
        if let Err(reason) = self.ensure_ready().await {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(reason),
                }],
            };
        }
        let own_ip = *self.external_ip.lock().await;
        if let Err(r) = guardrail::check(&entry.target, own_ip) {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(r),
                }],
            };
        }
        match nft::add_element(&*self.executor, &entry).await {
            Ok(()) => {
                let id = entry.id.clone();
                self.desired.lock().await.insert(entry.id.clone(), entry);
                AgentMessage::BlocklistAck {
                    results: vec![BlocklistAckItem {
                        id,
                        state: BlocklistEntryState::Present,
                        reason: None,
                    }],
                }
            }
            Err(e) => AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id: entry.id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(e.to_string()),
                }],
            },
        }
    }

    async fn handle_remove(&self, id: String) -> AgentMessage {
        let entry = self.desired.lock().await.get(&id).cloned();
        let Some(entry) = entry else {
            return AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id,
                    state: BlocklistEntryState::Absent,
                    reason: None,
                }],
            };
        };
        match nft::delete_element(&*self.executor, &entry).await {
            Ok(()) => {
                self.desired.lock().await.remove(&id);
                AgentMessage::BlocklistAck {
                    results: vec![BlocklistAckItem {
                        id,
                        state: BlocklistEntryState::Absent,
                        reason: None,
                    }],
                }
            }
            Err(e) => AgentMessage::BlocklistAck {
                results: vec![BlocklistAckItem {
                    id,
                    state: BlocklistEntryState::Failed,
                    reason: Some(e.to_string()),
                }],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firewall::nft::{NftError, NftOp};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct OkExec;
    #[async_trait]
    impl NftExecutor for OkExec {
        async fn run(&self, _: &[&str], _: NftOp) -> Result<(), NftError> {
            Ok(())
        }
        async fn list_json(&self, _: &[&str]) -> Result<String, NftError> {
            Ok(r#"{"nftables":[]}"#.into())
        }
    }

    struct FailAdd;
    #[async_trait]
    impl NftExecutor for FailAdd {
        async fn run(&self, _args: &[&str], op: NftOp) -> Result<(), NftError> {
            if matches!(op, NftOp::AddElement) {
                Err(NftError::PermissionDenied)
            } else {
                Ok(())
            }
        }
        async fn list_json(&self, _: &[&str]) -> Result<String, NftError> {
            Ok(r#"{"nftables":[]}"#.into())
        }
    }

    fn entry(id: &str, target: &str, family: u8) -> BlockEntry {
        BlockEntry {
            id: id.into(),
            target: target.into(),
            family,
        }
    }

    #[tokio::test]
    async fn add_success_inserts_into_desired() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].state, BlocklistEntryState::Present);
            }
            _ => panic!("expected ack"),
        }
        assert!(mgr.desired.lock().await.contains_key("b1"));
    }

    #[tokio::test]
    async fn failed_add_keeps_desired_clear_for_retry() {
        let mgr = FirewallManager::new(Arc::new(FailAdd), true);
        let ack = mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Failed);
            }
            _ => panic!(),
        }
        assert!(!mgr.desired.lock().await.contains_key("b1"));
    }

    #[tokio::test]
    async fn sync_acks_every_incoming_entry() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let entries = vec![entry("a", "1.1.1.1/32", 4), entry("b", "2.2.2.2/32", 4)];
        let ack = mgr.handle_sync(entries).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results.len(), 2);
                assert!(
                    results
                        .iter()
                        .all(|r| r.state == BlocklistEntryState::Present)
                );
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn reset_clears_desired_and_nft_ready() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        mgr.handle_add(entry("b1", "1.2.3.4/32", 4)).await;
        assert!(mgr.desired.lock().await.contains_key("b1"));
        let ack = mgr.handle_reset().await;
        assert!(matches!(
            ack,
            AgentMessage::BlocklistResetAck { ok: true, .. }
        ));
        assert!(mgr.desired.lock().await.is_empty());
        assert!(!*mgr.nft_ready.lock().await);
    }

    #[tokio::test]
    async fn guardrail_blocks_loopback() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_add(entry("b1", "127.0.0.1/32", 4)).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Failed);
                assert!(results[0].reason.as_ref().unwrap().contains("guardrail"));
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn remove_unknown_id_acks_absent() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        let ack = mgr.handle_remove("unknown".into()).await;
        match ack {
            AgentMessage::BlocklistAck { results } => {
                assert_eq!(results[0].state, BlocklistEntryState::Absent);
            }
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn sync_removes_orphans() {
        let mgr = FirewallManager::new(Arc::new(OkExec), true);
        mgr.handle_add(entry("orphan", "9.9.9.9/32", 4)).await;
        mgr.handle_sync(vec![entry("new", "1.1.1.1/32", 4)]).await;
        let g = mgr.desired.lock().await;
        assert!(!g.contains_key("orphan"));
        assert!(g.contains_key("new"));
    }
}
