mod capability_policy;
mod collector;
mod config;
mod docker;
mod file_manager;
mod fingerprint;
mod network_prober;
mod pinger;
mod probe_utils;
mod rebind;
mod register;
mod reporter;
mod terminal;

use std::sync::OnceLock;

use tracing_subscriber::EnvFilter;

use crate::capability_policy::{compute_agent_local_capabilities, parse_capability_args};
use crate::config::AgentConfig;
use crate::reporter::Reporter;

static RUSTLS_PROVIDER_INSTALLED: OnceLock<()> = OnceLock::new();

fn install_rustls_crypto_provider() -> anyhow::Result<()> {
    if RUSTLS_PROVIDER_INSTALLED.get().is_some() {
        return Ok(());
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls ring CryptoProvider"))?;

    let _ = RUSTLS_PROVIDER_INSTALLED.set(());
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = AgentConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {e}");
        eprintln!("Please create agent.toml or /etc/serverbee/agent.toml");
        std::process::exit(1);
    });
    let capability_overrides = parse_capability_args(std::env::args())?;
    let agent_local_capabilities = compute_agent_local_capabilities(&capability_overrides);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log.level.parse().unwrap_or_else(|_| "info".into())),
        )
        .init();

    install_rustls_crypto_provider()?;

    tracing::info!(
        "ServerBee Agent v{} starting...",
        serverbee_common::constants::VERSION
    );

    let machine_fingerprint = fingerprint::generate();
    if !machine_fingerprint.is_empty() {
        tracing::info!(
            "Machine fingerprint: {}...{}",
            &machine_fingerprint[..8],
            &machine_fingerprint[56..]
        );
    }

    if config.token.is_empty() {
        if config.auto_discovery_key.is_empty() {
            anyhow::bail!("No token and no auto_discovery_key. Set one in config.");
        }
        tracing::info!("No token found, registering...");
        let (_server_id, token) = register::register_agent(&config, &machine_fingerprint).await?;
        tracing::info!("Registration successful");
        if let Err(e) = register::save_token(&token) {
            tracing::warn!("Failed to save token: {e}");
        }
        config.token = token;
    }

    let mut reporter = Reporter::new(config, machine_fingerprint, agent_local_capabilities);
    reporter.run().await;
    Ok(())
}

#[cfg(test)]
#[test]
fn persist_rebind_token() {
    crate::rebind::assert_persist_rebind_token();
}

#[cfg(test)]
#[test]
fn config_path() {
    crate::config::assert_config_path();
}

#[cfg(test)]
mod tests {
    use super::install_rustls_crypto_provider;

    #[test]
    fn install_rustls_crypto_provider_is_idempotent() {
        install_rustls_crypto_provider().expect("first install should succeed");
        install_rustls_crypto_provider().expect("second install should be a no-op");
    }
}
