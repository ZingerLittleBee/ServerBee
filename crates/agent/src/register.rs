use std::time::Duration;

use anyhow::Result;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

#[cfg(not(test))]
use crate::rebind::persist_rebind_token;
#[cfg(test)]
use crate::rebind::persist_rebind_token_impl as persist_rebind_token;

/// Exit code reported to systemd when the agent gives up because of a
/// permanent registration error (bad/used/expired enrollment code). Matches
/// LSB "configuration error" (78) and is paired with
/// `RestartPreventExitStatus=78` in the systemd unit so we don't burn through
/// the rate-limit window with hopeless retries.
pub const EXIT_CODE_PERMANENT_AUTH_FAILURE: i32 = 78;

/// In-process retry cap before bailing back to systemd. Keeps the agent
/// trying through transient outages without spamming the rate-limit window
/// on permanent misconfig.
const MAX_REGISTER_ATTEMPTS: u32 = 30;
const INITIAL_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
const DEFAULT_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(60);

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

/// Categorized registration failure. Callers use this to decide between
/// retrying in-process (transient), waiting then retrying (rate-limited), or
/// giving up immediately (permanent).
#[derive(Debug)]
pub enum RegisterError {
    /// HTTP 401/403 — enrollment code is invalid, expired, or already used.
    /// Operator must rotate the code; retrying will never succeed.
    PermanentAuth(String),
    /// HTTP 429 — server is throttling this IP. Honor `Retry-After`.
    RateLimited {
        retry_after: Duration,
        message: String,
    },
    /// 5xx, network error, or non-401 4xx — likely transient.
    Transient(String),
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PermanentAuth(m) => write!(f, "permanent auth failure: {m}"),
            Self::RateLimited { retry_after, message } => write!(
                f,
                "rate limited (retry after {}s): {message}",
                retry_after.as_secs()
            ),
            Self::Transient(m) => write!(f, "transient error: {m}"),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Single attempt at registration. Used by both the bootstrap loop and the
/// reporter's mid-session rebind path. Errors are categorized so callers can
/// retry intelligently.
pub async fn register_agent(config: &AgentConfig, fingerprint: &str) -> Result<(String, String)> {
    register_once(config, fingerprint).await.map_err(|e| {
        anyhow::anyhow!(
            "Registration failed: {e}. \
             Check that the enrollment code is valid and not expired or already used."
        )
    })
}

async fn register_once(
    config: &AgentConfig,
    fingerprint: &str,
) -> Result<(String, String), RegisterError> {
    let url = format!(
        "{}/api/agent/register",
        config.server_url.trim_end_matches('/')
    );
    let client = reqwest::Client::new();
    let resp = match client
        .post(&url)
        .bearer_auth(&config.enrollment_code)
        .json(&RegisterRequest {
            fingerprint: fingerprint.to_string(),
        })
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return Err(RegisterError::Transient(format!("network error: {e}"))),
    };

    let status = resp.status();
    if status.is_success() {
        let data: RegisterResponse = resp
            .json()
            .await
            .map_err(|e| RegisterError::Transient(format!("invalid response body: {e}")))?;
        tracing::info!("Registered as server_id={}", data.data.server_id);
        return Ok((data.data.server_id, data.data.token));
    }

    let retry_after = parse_retry_after(&resp);
    let body = resp.text().await.unwrap_or_default();
    let trimmed = body.trim();
    let msg = format!("HTTP {status}. Server said: {trimmed}");

    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(RegisterError::PermanentAuth(msg)),
        StatusCode::TOO_MANY_REQUESTS => Err(RegisterError::RateLimited {
            retry_after: retry_after.unwrap_or(DEFAULT_RATE_LIMIT_BACKOFF),
            message: msg,
        }),
        _ => Err(RegisterError::Transient(msg)),
    }
}

fn parse_retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let raw = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    // Only the delta-seconds form is supported. HTTP-date form is rare in
    // practice and adds a date-parsing dep — caller falls back to a sane default.
    raw.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Retry registration with category-aware backoff. Returns immediately on
/// permanent auth failure so the caller can exit with
/// [`EXIT_CODE_PERMANENT_AUTH_FAILURE`].
///
/// - 401/403 → `Err(PermanentAuth)` immediately, no retry
/// - 429     → sleep `Retry-After` (or 60s), then retry
/// - 5xx/net → exponential backoff (5s, 10s, 20s, ..., capped at 5min)
///
/// Caps at [`MAX_REGISTER_ATTEMPTS`] in-process retries so we eventually
/// surface failures to systemd instead of looping forever.
pub async fn register_agent_with_backoff(
    config: &AgentConfig,
    fingerprint: &str,
) -> Result<(String, String), RegisterError> {
    let mut backoff = INITIAL_BACKOFF;
    for attempt in 1..=MAX_REGISTER_ATTEMPTS {
        match register_once(config, fingerprint).await {
            Ok(v) => return Ok(v),
            Err(RegisterError::PermanentAuth(msg)) => {
                tracing::error!(
                    "Permanent registration failure on attempt {attempt}: {msg}. \
                     Rotate the enrollment code in the server UI and restart the agent."
                );
                return Err(RegisterError::PermanentAuth(msg));
            }
            Err(RegisterError::RateLimited { retry_after, message }) => {
                let wait = retry_after.min(MAX_BACKOFF);
                tracing::warn!(
                    "Registration rate-limited on attempt {attempt}/{MAX_REGISTER_ATTEMPTS}: {message}. \
                     Sleeping {}s before retry.",
                    wait.as_secs()
                );
                tokio::time::sleep(wait).await;
            }
            Err(RegisterError::Transient(msg)) => {
                tracing::warn!(
                    "Transient registration error on attempt {attempt}/{MAX_REGISTER_ATTEMPTS}: {msg}. \
                     Sleeping {}s before retry.",
                    backoff.as_secs()
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
    Err(RegisterError::Transient(format!(
        "exhausted {MAX_REGISTER_ATTEMPTS} in-process retries"
    )))
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
