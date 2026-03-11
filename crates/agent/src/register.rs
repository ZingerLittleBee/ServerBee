use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

#[derive(Serialize)]
struct RegisterRequest {}

#[derive(Deserialize)]
struct RegisterResponse {
    data: RegisterData,
}

#[derive(Deserialize)]
struct RegisterData {
    server_id: String,
    token: String,
}

pub async fn register_agent(config: &AgentConfig) -> Result<(String, String)> {
    let url = format!(
        "{}/api/agent/register",
        config.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&config.auto_discovery_key)
        .json(&RegisterRequest {})
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("Registration failed: HTTP {}", resp.status());
    }
    let data: RegisterResponse = resp.json().await?;
    Ok((data.data.server_id, data.data.token))
}

pub fn save_token(token: &str) -> Result<()> {
    let path = AgentConfig::config_path();
    let content = if std::path::Path::new(path).exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };

    let mut lines: Vec<String> = content.lines().map(String::from).collect();
    let token_line = format!("token = \"{token}\"");
    if let Some(pos) = lines.iter().position(|l| l.starts_with("token")) {
        lines[pos] = token_line;
    } else {
        lines.push(token_line);
    }
    std::fs::write(path, lines.join("\n"))?;
    Ok(())
}
