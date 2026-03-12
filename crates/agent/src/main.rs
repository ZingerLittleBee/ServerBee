mod collector;
mod config;
mod pinger;
mod register;
mod reporter;
mod terminal;

use tracing_subscriber::EnvFilter;

use crate::config::AgentConfig;
use crate::reporter::Reporter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut config = AgentConfig::load().unwrap_or_else(|e| {
        eprintln!("Failed to load config: {e}");
        eprintln!("Please create agent.toml or /etc/serverbee/agent.toml");
        std::process::exit(1);
    });

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log.level.parse().unwrap_or_else(|_| "info".into())),
        )
        .init();

    tracing::info!(
        "ServerBee Agent v{} starting...",
        serverbee_common::constants::VERSION
    );

    if config.token.is_empty() {
        if config.auto_discovery_key.is_empty() {
            anyhow::bail!("No token and no auto_discovery_key. Set one in config.");
        }
        tracing::info!("No token found, registering...");
        let (_server_id, token) = register::register_agent(&config).await?;
        tracing::info!("Registration successful");
        if let Err(e) = register::save_token(&token) {
            tracing::warn!("Failed to save token: {e}");
        }
        config.token = token;
    }

    let mut reporter = Reporter::new(config);
    reporter.run().await;
    Ok(())
}
