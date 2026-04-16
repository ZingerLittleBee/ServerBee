use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

#[cfg(test)]
use crate::rebind::persist_rebind_token_impl as persist_rebind_token;
#[cfg(not(test))]
use crate::rebind::persist_rebind_token;

#[derive(Serialize)]
struct RegisterRequest {
    #[serde(skip_serializing_if = "String::is_empty")]
    fingerprint: String,
}

#[derive(Deserialize)]
struct RegisterResponse {
    data: RegisterData,
}

#[derive(Deserialize)]
struct RegisterData {
    server_id: String,
    token: String,
}

pub async fn register_agent(config: &AgentConfig, fingerprint: &str) -> Result<(String, String)> {
    let url = format!(
        "{}/api/agent/register",
        config.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&config.auto_discovery_key)
        .json(&RegisterRequest {
            fingerprint: fingerprint.to_string(),
        })
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Registration failed: HTTP {}", resp.status());
    }
    let data: RegisterResponse = resp.json().await?;
    Ok((data.data.server_id, data.data.token))
}

pub fn save_token(token: &str) -> Result<()> {
    persist_rebind_token(AgentConfig::config_path_for_persistence(), token)
}
