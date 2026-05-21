//! Firewall blocklist executor for the agent.

pub mod guardrail;
pub mod manager;
pub mod nft;

pub use manager::FirewallManager;

use std::sync::Arc;

use crate::firewall::nft::{CliNftExecutor, NftExecutor, NftOp};

/// Probe whether the host can actually execute firewall ops.
///
/// Runs a read-only `nft list ruleset`, then attempts to add and immediately
/// delete a throwaway table. Any failure (binary missing, kernel module
/// unavailable, lack of privileges) returns `false`. Slow path; call once at
/// startup.
pub async fn probe_local_capability() -> bool {
    let exec: Arc<dyn NftExecutor> = Arc::new(CliNftExecutor);
    if exec.list_json(&["ruleset"]).await.is_err() {
        return false;
    }
    if exec
        .run(
            &["add", "table", "inet", "serverbee_probe"],
            NftOp::AddTable,
        )
        .await
        .is_err()
    {
        return false;
    }
    let _ = exec
        .run(
            &["delete", "table", "inet", "serverbee_probe"],
            NftOp::DeleteTable,
        )
        .await;
    true
}
