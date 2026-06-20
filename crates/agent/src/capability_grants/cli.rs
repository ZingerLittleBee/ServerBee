use anyhow::{bail, Context};

use serverbee_common::constants::CapabilityKey;

use super::parse_duration_secs;
use super::store::{CapabilityGrantStore, GrantRecord};
use crate::config::AgentConfig;

pub struct GrantArgs {
    pub cap: String,
    pub for_duration: String,
    pub reason: Option<String>,
}

/// Temporarily enable `cap`. `base` is the agent's permanent (config-computed)
/// capability set; granting an already-on cap is rejected.
pub fn run_grant(
    config: &AgentConfig,
    base: u32,
    args: &GrantArgs,
    now: i64,
    granted_by: String,
) -> anyhow::Result<String> {
    let key: CapabilityKey = args.cap.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    if base & key.to_bit() != 0 {
        bail!("'{}' is already enabled in agent.toml; nothing to grant", key.as_str());
    }
    let dur = parse_duration_secs(&args.for_duration)?;
    let max = config.capabilities.temporary_max_duration_secs()?;
    if dur > max {
        bail!(
            "duration '{}' exceeds temporary_max_duration ('{}'); refusing",
            args.for_duration,
            config.capabilities.temporary_max_duration
        );
    }
    let path = config.capabilities.grants_path();
    let mut store = CapabilityGrantStore::load(&path);
    store.upsert(
        GrantRecord {
            cap: key.as_str().to_string(),
            granted_at: now,
            expires_at: now + dur,
            granted_by,
            reason: args.reason.clone(),
        },
        now,
    );
    store
        .flush()
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(format!(
        "Granted '{}' for {} (expires_at epoch {}). The running agent applies it within a few seconds.",
        key.as_str(),
        args.for_duration,
        now + dur
    ))
}

pub fn run_revoke(config: &AgentConfig, cap: &str, now: i64) -> anyhow::Result<String> {
    let key: CapabilityKey = cap.parse().map_err(|e: String| anyhow::anyhow!(e))?;
    let path = config.capabilities.grants_path();
    let mut store = CapabilityGrantStore::load(&path);
    let existed = store.remove(key.as_str(), now);
    store
        .flush()
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(if existed {
        format!("Revoked temporary grant for '{}'.", key.as_str())
    } else {
        format!("No active temporary grant for '{}'.", key.as_str())
    })
}

pub fn run_list(config: &AgentConfig, now: i64) -> anyhow::Result<String> {
    let store = CapabilityGrantStore::load(config.capabilities.grants_path());
    let mut lines: Vec<String> = store
        .records()
        .filter(|r| r.expires_at > now)
        .map(|r| {
            let reason = r
                .reason
                .as_deref()
                .map(|s| format!("  ({s})"))
                .unwrap_or_default();
            format!(
                "{:<16} expires in {:>7}s  by {}{}",
                r.cap,
                r.expires_at - now,
                r.granted_by,
                reason
            )
        })
        .collect();
    lines.sort();
    Ok(if lines.is_empty() {
        "No active temporary capability grants.".to_string()
    } else {
        lines.join("\n")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::constants::{CAP_DEFAULT, CAP_TERMINAL};

    fn config_with(dir: &std::path::Path) -> AgentConfig {
        let mut c = AgentConfig::default();
        c.capabilities.state_dir = dir.to_string_lossy().to_string();
        c.capabilities.temporary_max_duration = "24h".to_string();
        c
    }

    #[test]
    fn grant_then_revoke_round_trip() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let config = config_with(&dir);

        let out = run_grant(
            &config,
            CAP_DEFAULT,
            &GrantArgs { cap: "terminal".into(), for_duration: "30m".into(), reason: Some("x".into()) },
            1000,
            "root".into(),
        )
        .unwrap();
        assert!(out.contains("Granted 'terminal'"));

        let store = CapabilityGrantStore::load(config.capabilities.grants_path());
        assert_eq!(store.active_bits(1000, CAP_DEFAULT), CAP_TERMINAL);

        let out = run_revoke(&config, "terminal", 1001).unwrap();
        assert!(out.contains("Revoked"));
        let store = CapabilityGrantStore::load(config.capabilities.grants_path());
        assert_eq!(store.active_bits(1001, CAP_DEFAULT), 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grant_rejects_already_on_cap() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli2-{}", std::process::id()));
        let config = config_with(&dir);
        let err = run_grant(
            &config,
            CAP_DEFAULT | CAP_TERMINAL,
            &GrantArgs { cap: "terminal".into(), for_duration: "30m".into(), reason: None },
            0,
            "root".into(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("already enabled"));
    }

    #[test]
    fn grant_rejects_over_max_duration() {
        let dir = std::env::temp_dir().join(format!("sbtest-cli3-{}", std::process::id()));
        let config = config_with(&dir);
        let err = run_grant(
            &config,
            CAP_DEFAULT,
            &GrantArgs { cap: "terminal".into(), for_duration: "2d".into(), reason: None },
            0,
            "root".into(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("exceeds temporary_max_duration"));
    }
}
