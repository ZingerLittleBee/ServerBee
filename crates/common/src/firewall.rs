//! Wire types for the firewall blocklist feature. Shared between server
//! (source of truth) and agent (executor).

use serde::{Deserialize, Serialize};

/// Protocol-version gate: agents reporting a lower version must not receive
/// any `Blocklist*` or `BlocklistReset` messages.
pub const FIREWALL_MIN_PROTOCOL: u32 = 2;

/// Hard-coded protected CIDR ranges. These are refused by both the server
/// (tier 1 guardrail) and the agent (tier 3 guardrail). Defense-in-depth:
/// adding a CIDR here protects both sides automatically.
pub const PROTECTED_CIDRS: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
    "::/128",
];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct BlockEntry {
    pub id: String,
    /// Canonical IpNet string (`1.2.3.4/32`, `10.0.0.0/8`, `2001:db8::/32`).
    pub target: String,
    /// `4` or `6`.
    pub family: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BlocklistEntryState {
    Present,
    Absent,
    Failed,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct BlocklistAckItem {
    pub id: String,
    pub state: BlocklistEntryState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BlocklistChangeKind {
    Created,
    Deleted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_snake_case() {
        let json = serde_json::to_string(&BlocklistEntryState::Present).unwrap();
        assert_eq!(json, "\"present\"");
        let parsed: BlocklistEntryState = serde_json::from_str("\"failed\"").unwrap();
        assert_eq!(parsed, BlocklistEntryState::Failed);
    }

    #[test]
    fn ack_item_skips_none_reason() {
        let item = BlocklistAckItem {
            id: "id-1".into(),
            state: BlocklistEntryState::Present,
            reason: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(!json.contains("reason"));
    }
}
