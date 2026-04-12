use anyhow::bail;
use serverbee_common::constants::{CAP_DEFAULT, CAP_VALID_MASK, CapabilityKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityCliOverrides {
    pub allow_caps: Vec<CapabilityKey>,
    pub deny_caps: Vec<CapabilityKey>,
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

pub fn compute_agent_local_capabilities(overrides: &CapabilityCliOverrides) -> u32 {
    let mut capabilities = CAP_DEFAULT;

    for capability in &overrides.allow_caps {
        capabilities |= capability.to_bit();
    }

    for capability in &overrides.deny_caps {
        capabilities &= !capability.to_bit();
    }

    capabilities & CAP_VALID_MASK
}

fn is_unknown_capability_flag(arg: &str) -> bool {
    (arg.starts_with("--allow-cap") && arg != "--allow-cap")
        || (arg.starts_with("--deny-cap") && arg != "--deny-cap")
}

#[cfg(test)]
mod tests {
    use serverbee_common::constants::{
        CAP_DEFAULT, CAP_EXEC, CAP_FILE, CAP_PING_HTTP, CapabilityKey,
    };

    use super::{CapabilityCliOverrides, compute_agent_local_capabilities, parse_capability_args};

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
    fn test_compute_agent_local_capabilities_defaults_to_cap_default() {
        let overrides = CapabilityCliOverrides {
            allow_caps: vec![],
            deny_caps: vec![],
        };

        assert_eq!(compute_agent_local_capabilities(&overrides), CAP_DEFAULT);
    }

    #[test]
    fn test_compute_agent_local_capabilities_applies_allow_and_deny_with_deny_winning() {
        let overrides = CapabilityCliOverrides {
            allow_caps: vec![
                CapabilityKey::File,
                CapabilityKey::Exec,
                CapabilityKey::File,
            ],
            deny_caps: vec![CapabilityKey::PingHttp, CapabilityKey::Exec],
        };

        assert_eq!(
            compute_agent_local_capabilities(&overrides),
            (CAP_DEFAULT | CAP_FILE) & !CAP_PING_HTTP
        );
        assert_eq!(compute_agent_local_capabilities(&overrides) & CAP_EXEC, 0);
        assert_eq!(
            compute_agent_local_capabilities(&overrides) & CAP_PING_HTTP,
            0
        );
    }
}
