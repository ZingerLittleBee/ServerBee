use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

#[cfg(not(test))]
use crate::rebind::persist_rebind_token;
#[cfg(test)]
use crate::rebind::persist_rebind_token_impl as persist_rebind_token;

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
    if AgentConfig::token_env_override_present() {
        anyhow::bail!("SERVERBEE_TOKEN is set; refusing to persist token to agent.toml");
    }

    persist_rebind_token(AgentConfig::config_path_for_persistence(), token)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use tempfile::TempDir;

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    fn set_current_dir(dir: &TempDir) -> CurrentDirGuard {
        let original = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("set cwd");
        CurrentDirGuard { original }
    }

    #[test]
    fn save_token_rejects_persistence_when_serverbee_token_is_set() {
        crate::config::with_serverbee_token_env(Some("env-token"), || {
            let tempdir = TempDir::new().expect("tempdir");
            let _cwd_guard = set_current_dir(&tempdir);

            let result = super::save_token("persisted-token");

            let err = result.expect_err("save_token should fail");
            assert!(
                err.to_string().contains("SERVERBEE_TOKEN"),
                "unexpected error: {err}"
            );
            assert!(
                !tempdir.path().join("agent.toml").exists(),
                "token persistence should not write a config file"
            );
        });
    }

    #[test]
    fn save_token_allows_persistence_when_serverbee_token_is_unset() {
        crate::config::with_serverbee_token_env(None, || {
            let tempdir = TempDir::new().expect("tempdir");
            let _cwd_guard = set_current_dir(&tempdir);

            super::save_token("persisted-token").expect("save_token");

            let content = fs::read_to_string(tempdir.path().join("agent.toml"))
                .expect("read persisted config");
            assert!(
                content.contains("token = \"persisted-token\""),
                "expected persisted token, got: {content}"
            );
        });
    }
}
