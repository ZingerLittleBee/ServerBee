//! `nft` CLI driver. The trait lets tests mock subprocess invocations.

use async_trait::async_trait;
use serverbee_common::firewall::BlockEntry;
use tokio::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum NftError {
    #[error("permission denied — needs root or CAP_NET_ADMIN")]
    PermissionDenied,
    #[error("nft kernel module unavailable")]
    KernelMissing,
    #[error("nft cli not found in PATH")]
    BinaryMissing,
    #[error("{0}")]
    Other(String),
}

#[derive(Copy, Clone, Debug)]
pub enum NftOp {
    AddTable,
    AddSet,
    AddChain,
    AddRule,
    AddElement,
    DeleteElement,
    FlushSet,
    DeleteTable,
}

/// Returns true when stderr indicates the kernel/library considers the
/// requested element to already be in the desired state (EEXIST on add,
/// ENOENT on delete/flush). These are mapped to success by the manager.
pub fn is_idempotent_signal(stderr: &str, op: NftOp) -> bool {
    match op {
        NftOp::AddElement | NftOp::AddTable | NftOp::AddSet | NftOp::AddChain | NftOp::AddRule => {
            stderr.contains("File exists")
        }
        NftOp::DeleteElement | NftOp::FlushSet | NftOp::DeleteTable => {
            stderr.contains("No such file or directory")
        }
    }
}

#[async_trait]
pub trait NftExecutor: Send + Sync {
    async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError>;
    async fn list_json(&self, args: &[&str]) -> Result<String, NftError>;
}

pub struct CliNftExecutor;

#[async_trait]
impl NftExecutor for CliNftExecutor {
    async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError> {
        let out = Command::new("nft")
            .args(args)
            .output()
            .await
            .map_err(|e| {
                if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                    NftError::BinaryMissing
                } else {
                    NftError::Other(e.to_string())
                }
            })?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        if is_idempotent_signal(&stderr, op) {
            return Ok(());
        }
        if stderr.contains("Operation not permitted") {
            return Err(NftError::PermissionDenied);
        }
        if stderr.contains("No such file or directory") {
            // Resource op without an idempotence signal — kernel module probably missing.
            return Err(NftError::KernelMissing);
        }
        Err(NftError::Other(
            stderr.lines().next().unwrap_or("nft failed").to_string(),
        ))
    }

    async fn list_json(&self, args: &[&str]) -> Result<String, NftError> {
        let mut full = vec!["-j", "list"];
        full.extend_from_slice(args);
        let out = Command::new("nft")
            .args(&full)
            .output()
            .await
            .map_err(|e| {
                if matches!(e.kind(), std::io::ErrorKind::NotFound) {
                    NftError::BinaryMissing
                } else {
                    NftError::Other(e.to_string())
                }
            })?;
        if !out.status.success() {
            return Err(NftError::Other(
                String::from_utf8_lossy(&out.stderr).to_string(),
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

/// High-level operations the manager calls. Implemented as free functions
/// taking `&dyn NftExecutor` so tests can swap.
pub async fn ensure_resources(exec: &dyn NftExecutor) -> Result<(), NftError> {
    exec.run(&["add", "table", "inet", "serverbee"], NftOp::AddTable)
        .await?;
    exec.run(
        &[
            "add",
            "set",
            "inet",
            "serverbee",
            "block_v4",
            "{ type ipv4_addr; flags interval; }",
        ],
        NftOp::AddSet,
    )
    .await?;
    exec.run(
        &[
            "add",
            "set",
            "inet",
            "serverbee",
            "block_v6",
            "{ type ipv6_addr; flags interval; }",
        ],
        NftOp::AddSet,
    )
    .await?;
    exec.run(
        &[
            "add",
            "chain",
            "inet",
            "serverbee",
            "input",
            "{ type filter hook input priority -10; }",
        ],
        NftOp::AddChain,
    )
    .await?;
    // Add the two drop rules; `nft add rule` is not idempotent natively, so
    // detect them in the existing ruleset first.
    let listing = exec
        .list_json(&["chain", "inet", "serverbee", "input"])
        .await?;
    if !chain_has_set_drop_rule(&listing, "block_v4") {
        exec.run(
            &[
                "add", "rule", "inet", "serverbee", "input", "ip", "saddr", "@block_v4", "drop",
            ],
            NftOp::AddRule,
        )
        .await?;
    }
    if !chain_has_set_drop_rule(&listing, "block_v6") {
        exec.run(
            &[
                "add", "rule", "inet", "serverbee", "input", "ip6", "saddr", "@block_v6", "drop",
            ],
            NftOp::AddRule,
        )
        .await?;
    }
    Ok(())
}

/// Returns true when the `nft -j list chain ...` output contains a rule
/// matching against the named set. Parsed structurally instead of via
/// substring matching so reformatted JSON or unrelated string occurrences
/// can't false-positive or false-negative the check.
fn chain_has_set_drop_rule(listing: &str, set_name: &str) -> bool {
    let v: serde_json::Value = match serde_json::from_str(listing) {
        Ok(v) => v,
        Err(_) => return false, // treat malformed listing as "rule missing" → safe to add
    };
    let items = v
        .get("nftables")
        .and_then(|n| n.as_array())
        .cloned()
        .unwrap_or_default();
    for item in items {
        let Some(rule) = item.get("rule") else {
            continue;
        };
        let Some(expr) = rule.get("expr").and_then(|e| e.as_array()) else {
            continue;
        };
        for stmt in expr {
            if let Some(m) = stmt.get("match")
                && let Some(set) = m.get("right").and_then(|r| r.get("set"))
                && set.as_str() == Some(set_name)
            {
                return true;
            }
        }
    }
    false
}

pub async fn add_element(exec: &dyn NftExecutor, entry: &BlockEntry) -> Result<(), NftError> {
    let set = if entry.family == 4 {
        "block_v4"
    } else {
        "block_v6"
    };
    let arg = format!("{{ {} }}", entry.target);
    exec.run(
        &["add", "element", "inet", "serverbee", set, &arg],
        NftOp::AddElement,
    )
    .await
}

pub async fn delete_element(exec: &dyn NftExecutor, entry: &BlockEntry) -> Result<(), NftError> {
    let set = if entry.family == 4 {
        "block_v4"
    } else {
        "block_v6"
    };
    let arg = format!("{{ {} }}", entry.target);
    exec.run(
        &["delete", "element", "inet", "serverbee", set, &arg],
        NftOp::DeleteElement,
    )
    .await
}

pub async fn unconditional_wipe(exec: &dyn NftExecutor) -> Result<(), NftError> {
    let _ = exec
        .run(
            &["flush", "set", "inet", "serverbee", "block_v4"],
            NftOp::FlushSet,
        )
        .await;
    let _ = exec
        .run(
            &["flush", "set", "inet", "serverbee", "block_v6"],
            NftOp::FlushSet,
        )
        .await;
    // Delete the whole table so a fresh resource bootstrap happens next time.
    exec.run(
        &["delete", "table", "inet", "serverbee"],
        NftOp::DeleteTable,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct MockExec {
        calls: Mutex<Vec<Vec<String>>>,
        respond_eexist_on_add_table: Mutex<bool>,
    }

    #[async_trait]
    impl NftExecutor for MockExec {
        async fn run(&self, args: &[&str], op: NftOp) -> Result<(), NftError> {
            self.calls
                .lock()
                .await
                .push(args.iter().map(|s| s.to_string()).collect());
            // Simulate "table already exists" idempotence.
            if matches!(op, NftOp::AddTable) && *self.respond_eexist_on_add_table.lock().await {
                return Ok(());
            }
            Ok(())
        }
        async fn list_json(&self, _args: &[&str]) -> Result<String, NftError> {
            // Pretend the chain has no rules yet (real nft shape).
            Ok(r#"{"nftables":[]}"#.into())
        }
    }

    #[tokio::test]
    async fn ensure_resources_runs_all_steps() {
        let exec = MockExec::default();
        ensure_resources(&exec).await.unwrap();
        let calls = exec.calls.lock().await;
        let has = |needle: &str| calls.iter().any(|c| c.join(" ").contains(needle));
        assert!(has("add table inet serverbee"));
        assert!(has("add set inet serverbee block_v4"));
        assert!(has("add chain inet serverbee input"));
        assert!(has("add rule inet serverbee input ip saddr @block_v4 drop"));
    }

    #[tokio::test]
    async fn add_element_v4_uses_v4_set() {
        let exec = MockExec::default();
        let entry = BlockEntry {
            id: "x".into(),
            target: "1.2.3.4/32".into(),
            family: 4,
        };
        add_element(&exec, &entry).await.unwrap();
        let calls = exec.calls.lock().await;
        let joined = calls[0].join(" ");
        assert!(joined.contains("block_v4"));
        assert!(joined.contains("1.2.3.4/32"));
    }

    #[test]
    fn eexist_classified_as_idempotent_add() {
        assert!(is_idempotent_signal(
            "Error: File exists",
            NftOp::AddElement
        ));
    }

    #[test]
    fn enoent_classified_as_idempotent_delete() {
        assert!(is_idempotent_signal(
            "Error: No such file or directory",
            NftOp::DeleteElement
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_detects_rule() {
        let listing = r#"{"nftables":[{"rule":{"expr":[{"match":{"left":{},"op":"==","right":{"set":"block_v4"}}},{"drop":null}]}}]}"#;
        assert!(super::chain_has_set_drop_rule(listing, "block_v4"));
        assert!(!super::chain_has_set_drop_rule(listing, "block_v6"));
    }

    #[test]
    fn chain_has_set_drop_rule_handles_empty() {
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":[]}"#,
            "block_v4"
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_handles_malformed() {
        assert!(!super::chain_has_set_drop_rule("not json", "block_v4"));
    }

    // ----- is_idempotent_signal branch coverage -----

    #[test]
    fn add_op_without_file_exists_is_not_idempotent() {
        // Add op but stderr is some other failure → must NOT be treated as
        // already-in-desired-state.
        assert!(!is_idempotent_signal(
            "Error: Operation not permitted",
            NftOp::AddElement
        ));
    }

    #[test]
    fn delete_op_with_enoent_is_idempotent_for_all_delete_variants() {
        assert!(is_idempotent_signal(
            "Error: No such file or directory",
            NftOp::FlushSet
        ));
        assert!(is_idempotent_signal(
            "Error: No such file or directory",
            NftOp::DeleteTable
        ));
    }

    #[test]
    fn delete_op_with_file_exists_is_not_idempotent() {
        // ENOENT is the only idempotent signal for delete ops; a stray
        // "File exists" must not be mistaken for one.
        assert!(!is_idempotent_signal("Error: File exists", NftOp::DeleteElement));
    }

    #[test]
    fn add_table_set_chain_rule_share_eexist_signal() {
        assert!(is_idempotent_signal("File exists", NftOp::AddTable));
        assert!(is_idempotent_signal("File exists", NftOp::AddSet));
        assert!(is_idempotent_signal("File exists", NftOp::AddChain));
        assert!(is_idempotent_signal("File exists", NftOp::AddRule));
    }

    // ----- element op family selection -----

    #[tokio::test]
    async fn add_element_v6_uses_v6_set() {
        let exec = MockExec::default();
        let entry = BlockEntry {
            id: "y".into(),
            target: "2001:db8::/32".into(),
            family: 6,
        };
        add_element(&exec, &entry).await.unwrap();
        let calls = exec.calls.lock().await;
        let joined = calls[0].join(" ");
        assert!(joined.contains("block_v6"));
        assert!(joined.contains("2001:db8::/32"));
        assert!(joined.contains("add element inet serverbee"));
    }

    #[tokio::test]
    async fn delete_element_v4_uses_v4_set() {
        let exec = MockExec::default();
        let entry = BlockEntry {
            id: "z".into(),
            target: "5.6.7.8/32".into(),
            family: 4,
        };
        delete_element(&exec, &entry).await.unwrap();
        let calls = exec.calls.lock().await;
        let joined = calls[0].join(" ");
        assert!(joined.contains("delete element inet serverbee block_v4"));
        assert!(joined.contains("5.6.7.8/32"));
    }

    #[tokio::test]
    async fn delete_element_v6_uses_v6_set() {
        let exec = MockExec::default();
        let entry = BlockEntry {
            id: "z".into(),
            target: "2001:db8::1/128".into(),
            family: 6,
        };
        delete_element(&exec, &entry).await.unwrap();
        let calls = exec.calls.lock().await;
        let joined = calls[0].join(" ");
        assert!(joined.contains("delete element inet serverbee block_v6"));
    }

    // ----- unconditional_wipe -----

    #[tokio::test]
    async fn unconditional_wipe_flushes_both_sets_then_deletes_table() {
        let exec = MockExec::default();
        unconditional_wipe(&exec).await.unwrap();
        let calls = exec.calls.lock().await;
        let has = |needle: &str| calls.iter().any(|c| c.join(" ").contains(needle));
        assert!(has("flush set inet serverbee block_v4"));
        assert!(has("flush set inet serverbee block_v6"));
        assert!(has("delete table inet serverbee"));
        // delete table must be the final call.
        assert!(calls.last().unwrap().join(" ").contains("delete table"));
    }

    // ----- ensure_resources when drop rules already exist -----

    /// Mock whose `list_json` reports both drop rules already present, so
    /// `ensure_resources` must SKIP both `add rule` invocations.
    #[derive(Default)]
    struct RulesPresentExec {
        calls: Mutex<Vec<Vec<String>>>,
    }

    #[async_trait]
    impl NftExecutor for RulesPresentExec {
        async fn run(&self, args: &[&str], _op: NftOp) -> Result<(), NftError> {
            self.calls
                .lock()
                .await
                .push(args.iter().map(|s| s.to_string()).collect());
            Ok(())
        }
        async fn list_json(&self, _args: &[&str]) -> Result<String, NftError> {
            Ok(r#"{"nftables":[
                {"rule":{"expr":[{"match":{"right":{"set":"block_v4"}}},{"drop":null}]}},
                {"rule":{"expr":[{"match":{"right":{"set":"block_v6"}}},{"drop":null}]}}
            ]}"#
            .into())
        }
    }

    #[tokio::test]
    async fn ensure_resources_skips_existing_drop_rules() {
        let exec = RulesPresentExec::default();
        ensure_resources(&exec).await.unwrap();
        let calls = exec.calls.lock().await;
        let added_rule = calls.iter().any(|c| c.join(" ").contains("add rule"));
        assert!(!added_rule, "drop rules already present must not be re-added");
        // Table/set/chain bootstrap still happens.
        assert!(calls.iter().any(|c| c.join(" ").contains("add table inet serverbee")));
    }

    /// Mock whose first `run` (add table) fails, so `ensure_resources`
    /// propagates the error before reaching the listing step.
    struct AddTableFailsExec;

    #[async_trait]
    impl NftExecutor for AddTableFailsExec {
        async fn run(&self, _args: &[&str], op: NftOp) -> Result<(), NftError> {
            if matches!(op, NftOp::AddTable) {
                Err(NftError::PermissionDenied)
            } else {
                Ok(())
            }
        }
        async fn list_json(&self, _args: &[&str]) -> Result<String, NftError> {
            Ok(r#"{"nftables":[]}"#.into())
        }
    }

    #[tokio::test]
    async fn ensure_resources_propagates_run_error() {
        let exec = AddTableFailsExec;
        let err = ensure_resources(&exec).await.unwrap_err();
        assert!(matches!(err, NftError::PermissionDenied));
    }

    /// Mock whose `list_json` errors, exercising the `?` on the listing call.
    struct ListFailsExec;

    #[async_trait]
    impl NftExecutor for ListFailsExec {
        async fn run(&self, _args: &[&str], _op: NftOp) -> Result<(), NftError> {
            Ok(())
        }
        async fn list_json(&self, _args: &[&str]) -> Result<String, NftError> {
            Err(NftError::Other("list boom".into()))
        }
    }

    #[tokio::test]
    async fn ensure_resources_propagates_list_error() {
        let exec = ListFailsExec;
        let err = ensure_resources(&exec).await.unwrap_err();
        assert!(matches!(err, NftError::Other(_)));
    }

    // ----- chain_has_set_drop_rule structural parsing edge cases -----

    #[test]
    fn chain_has_set_drop_rule_missing_nftables_key() {
        assert!(!super::chain_has_set_drop_rule(r#"{"other":[]}"#, "block_v4"));
    }

    #[test]
    fn chain_has_set_drop_rule_nftables_not_array() {
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":{"k":"v"}}"#,
            "block_v4"
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_item_without_rule_is_skipped() {
        // First item has no "rule" key (e.g. a "table"/"chain" metadata item),
        // second carries the matching rule.
        let listing = r#"{"nftables":[
            {"table":{"name":"serverbee"}},
            {"rule":{"expr":[{"match":{"right":{"set":"block_v4"}}}]}}
        ]}"#;
        assert!(super::chain_has_set_drop_rule(listing, "block_v4"));
    }

    #[test]
    fn chain_has_set_drop_rule_rule_without_expr_is_skipped() {
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":[{"rule":{"family":"inet"}}]}"#,
            "block_v4"
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_stmt_without_match_is_skipped() {
        // expr present but its statements never carry a "match" → no hit.
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":[{"rule":{"expr":[{"drop":null}]}}]}"#,
            "block_v4"
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_match_without_set_right_is_skipped() {
        // match present but right side has no "set" key.
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":[{"rule":{"expr":[{"match":{"right":{"payload":{}}}}]}}]}"#,
            "block_v4"
        ));
    }

    #[test]
    fn chain_has_set_drop_rule_different_set_name_is_skipped() {
        // A rule that references a *different* set must not match.
        assert!(!super::chain_has_set_drop_rule(
            r#"{"nftables":[{"rule":{"expr":[{"match":{"right":{"set":"some_other_set"}}}]}}]}"#,
            "block_v4"
        ));
    }
}
