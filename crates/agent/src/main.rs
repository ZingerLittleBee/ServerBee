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
mod upgrade;

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

    if let Err(_e) = rustls::crypto::ring::default_provider().install_default() {
        // install_default() returns Err if a process-global provider is already installed
        // by another code path (e.g. reqwest building a ClientConfig in a parallel test).
        // If a provider is now present, treat that as success — we just didn't win the race.
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            return Err(anyhow::anyhow!("Failed to install rustls ring CryptoProvider"));
        }
    }

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
    if let Some(repo) = crate::upgrade::parse_release_repo_arg(std::env::args()) {
        tracing::info!("release_repo_url overridden by --release-repo CLI flag");
        config.upgrade.release_repo_url = repo;
    }
    let agent_local_capabilities = compute_agent_local_capabilities(&capability_overrides);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log.level.parse().unwrap_or_else(|_| "info".into())),
        )
        .init();

    install_rustls_crypto_provider()?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in ["agent.toml", "/etc/serverbee/agent.toml"] {
            if let Ok(meta) = std::fs::metadata(path) {
                let mode = meta.permissions().mode();
                if crate::upgrade::is_group_or_world_writable(mode) {
                    tracing::warn!(
                        "SECURITY: {path} is group/world-writable (mode {:o}); \
                         another local user could tamper release_repo_url. \
                         Run: chmod 600 {path}",
                        mode & 0o777
                    );
                }
            }
        }
    }

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
        if config.enrollment_code.is_empty() {
            anyhow::bail!(
                "No token and no enrollment_code. Generate a one-time code in the \
                 server UI (Settings) and set `enrollment_code` in agent.toml or the \
                 SERVERBEE_ENROLLMENT_CODE environment variable."
            );
        }
        tracing::info!("No token found, registering...");
        let (server_id, token) = register::register_agent(&config, &machine_fingerprint).await?;
        tracing::info!("Registration successful (server_id={server_id})");
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

    #[test]
    fn install_is_ok_when_provider_preinstalled() {
        // Simulate another code path (e.g. reqwest) installing the global provider first.
        // The result is intentionally ignored — it may fail if a provider is already set.
        let _ = rustls::crypto::ring::default_provider().install_default();
        // Our function must succeed even though install_default() would now return Err.
        install_rustls_crypto_provider()
            .expect("should succeed even when a provider was already installed");
        install_rustls_crypto_provider()
            .expect("second call should also succeed (OnceLock fast-path)");
    }
}
