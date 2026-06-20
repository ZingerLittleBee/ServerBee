use anyhow::bail;
use serverbee_common::constants::{CAP_DEFAULT, CAP_VALID_MASK, CapabilityKey};

use crate::config::CapabilitiesConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCliOverrides {
    pub allow_caps: Vec<CapabilityKey>,
    pub deny_caps: Vec<CapabilityKey>,
}

/// Parse a `[capabilities]` config block (string keys) into typed
/// allow/deny capability lists. Unknown keys are a hard error so a typo in
/// the agent config fails fast at startup rather than silently dropping a
/// capability the operator intended to enable.
fn parse_config_capabilities(
    config: &CapabilitiesConfig,
) -> anyhow::Result<CapabilityCliOverrides> {
    let parse_list = |keys: &[String]| -> anyhow::Result<Vec<CapabilityKey>> {
        keys.iter()
            .map(|key| key.parse::<CapabilityKey>().map_err(anyhow::Error::msg))
            .collect()
    };
    Ok(CapabilityCliOverrides {
        allow_caps: parse_list(&config.allow)?,
        deny_caps: parse_list(&config.deny)?,
    })
}

fn apply_overrides(mut capabilities: u32, overrides: &CapabilityCliOverrides) -> u32 {
    for capability in &overrides.allow_caps {
        capabilities |= capability.to_bit();
    }
    for capability in &overrides.deny_caps {
        capabilities &= !capability.to_bit();
    }
    capabilities
}

/// Compute the agent's local capability bitmask. The config file is the
/// source of truth (`[capabilities]` allow/deny over `CAP_DEFAULT`); CLI
/// `--allow-cap` / `--deny-cap` flags layer on top for ad-hoc overrides.
/// Within each layer `deny` wins; the CLI layer wins over the config layer.
pub fn compute_local_capabilities(
    config: &CapabilitiesConfig,
    cli: &CapabilityCliOverrides,
) -> anyhow::Result<u32> {
    let config_overrides = parse_config_capabilities(config)?;
    let capabilities = apply_overrides(CAP_DEFAULT, &config_overrides);
    let capabilities = apply_overrides(capabilities, cli);
    Ok(capabilities & CAP_VALID_MASK)
}

pub fn parse_capability_args<I>(args: I) -> anyhow::Result<CapabilityCliOverrides>
where
    I: IntoIterator<Item = String>,
{
    let mut allow_caps = Vec::new();
    let mut deny_caps = Vec::new();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--allow-cap" => {
                let value = match args.next() {
                    Some(value) if !value.starts_with("--") => value,
                    _ => bail!("missing value for --allow-cap"),
                };
                let capability = value.parse::<CapabilityKey>().map_err(anyhow::Error::msg)?;
                allow_caps.push(capability);
            }
            "--deny-cap" => {
                let value = match args.next() {
                    Some(value) if !value.starts_with("--") => value,
                    _ => bail!("missing value for --deny-cap"),
                };
                let capability = value.parse::<CapabilityKey>().map_err(anyhow::Error::msg)?;
                deny_caps.push(capability);
            }
            _ if is_unknown_capability_flag(&arg) => {
                bail!("unknown capability flag: {arg}");
            }
            _ => {}
        }
    }

    Ok(CapabilityCliOverrides {
        allow_caps,
        deny_caps,
    })
}

fn is_unknown_capability_flag(arg: &str) -> bool {
    (arg.starts_with("--allow-cap") && arg != "--allow-cap")
        || (arg.starts_with("--deny-cap") && arg != "--deny-cap")
}

#[cfg(test)]
mod tests {
    use serverbee_common::constants::{
        CAP_DEFAULT, CAP_DOCKER, CAP_EXEC, CAP_FILE, CAP_PING_HTTP, CAP_TERMINAL, CapabilityKey,
        has_capability,
    };

    use super::{CapabilityCliOverrides, compute_local_capabilities, parse_capability_args};
    use crate::config::CapabilitiesConfig;

    fn no_cli() -> CapabilityCliOverrides {
        CapabilityCliOverrides {
            allow_caps: vec![],
            deny_caps: vec![],
        }
    }

    #[test]
    fn test_parse_capability_args_ignores_unrelated_segments() {
        let overrides = parse_capability_args(vec![
            "serverbee-agent".to_string(),
            "--config".to_string(),
            "agent.toml".to_string(),
            "--allow-cap".to_string(),
            "file".to_string(),
            "--deny-cap".to_string(),
            "ping_http".to_string(),
        ])
        .expect("parser should accept known capability flags");

        assert_eq!(overrides.allow_caps, vec![CapabilityKey::File]);
        assert_eq!(overrides.deny_caps, vec![CapabilityKey::PingHttp]);
    }

    #[test]
    fn test_parse_capability_args_rejects_missing_allow_cap_value() {
        let error = parse_capability_args(vec![
            "serverbee-agent".to_string(),
            "--allow-cap".to_string(),
        ])
        .expect_err("missing allow value should fail");

        assert!(error.to_string().contains("missing value for --allow-cap"));
    }

    #[test]
    fn test_parse_capability_args_rejects_unknown_capability_like_flag() {
        let error = parse_capability_args(vec![
            "serverbee-agent".to_string(),
            "--allow-caps".to_string(),
            "file".to_string(),
        ])
        .expect_err("unknown capability-like flag should fail");

        assert!(error.to_string().contains("unknown capability flag"));
    }

    #[test]
    fn test_parse_capability_args_rejects_unknown_capability_name() {
        let error = parse_capability_args(vec![
            "serverbee-agent".to_string(),
            "--allow-cap".to_string(),
            "nope".to_string(),
        ])
        .expect_err("unknown capability name should fail");

        assert!(error.to_string().contains("unknown capability"));
    }

    #[test]
    fn test_empty_config_and_cli_defaults_to_cap_default() {
        let caps = compute_local_capabilities(&CapabilitiesConfig::default(), &no_cli())
            .expect("default config should compute");
        assert_eq!(caps, CAP_DEFAULT);
    }

    #[test]
    fn test_cli_applies_allow_and_deny_with_deny_winning() {
        let cli = CapabilityCliOverrides {
            allow_caps: vec![CapabilityKey::File, CapabilityKey::Exec, CapabilityKey::File],
            deny_caps: vec![CapabilityKey::PingHttp, CapabilityKey::Exec],
        };

        let caps = compute_local_capabilities(&CapabilitiesConfig::default(), &cli)
            .expect("config should compute");
        assert_eq!(caps, (CAP_DEFAULT | CAP_FILE) & !CAP_PING_HTTP);
        assert_eq!(caps & CAP_EXEC, 0);
        assert_eq!(caps & CAP_PING_HTTP, 0);
    }

    #[test]
    fn test_config_file_allow_and_deny_apply_over_default() {
        // The agent config file is the source of truth: allow adds high-risk
        // caps, deny strips defaults.
        let config = CapabilitiesConfig {
            allow: vec!["terminal".to_string(), "file".to_string()],
            deny: vec!["ip_quality".to_string()],
        };
        let caps =
            compute_local_capabilities(&config, &no_cli()).expect("config should compute");
        assert!(has_capability(caps, CAP_TERMINAL));
        assert!(has_capability(caps, CAP_FILE));
        assert!(!has_capability(caps, serverbee_common::constants::CAP_IP_QUALITY));
    }

    #[test]
    fn test_cli_layer_overrides_config_layer() {
        // Config allows docker; CLI denies it. CLI is applied last and wins.
        let config = CapabilitiesConfig {
            allow: vec!["docker".to_string()],
            deny: vec![],
        };
        let cli = CapabilityCliOverrides {
            allow_caps: vec![],
            deny_caps: vec![CapabilityKey::Docker],
        };
        let caps = compute_local_capabilities(&config, &cli).expect("config should compute");
        assert!(!has_capability(caps, CAP_DOCKER));
    }

    #[test]
    fn test_config_file_unknown_capability_key_is_rejected() {
        let config = CapabilitiesConfig {
            allow: vec!["definitely_not_a_cap".to_string()],
            deny: vec![],
        };
        let err = compute_local_capabilities(&config, &no_cli())
            .expect_err("unknown config capability key should fail");
        assert!(err.to_string().contains("unknown capability"));
    }
}
