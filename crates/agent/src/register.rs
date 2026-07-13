use std::path::Path;
use std::time::Duration;

use base64::Engine;
use rand::RngCore;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use crate::config::AgentConfig;

/// Matches LSB "configuration error" and the systemd
/// `RestartPreventExitStatus=78` policy used by the installer.
pub const EXIT_CODE_PERMANENT_AUTH_FAILURE: i32 = 78;

const MAX_REGISTER_ATTEMPTS: u32 = 30;
const MAX_BACKOFF: Duration = Duration::from_secs(300);
const DEFAULT_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Serialize)]
struct RegisterRequest<'a> {
    proposed_run_token: &'a str,
}

#[derive(Deserialize)]
struct RegisterResponse {
    data: RegisterData,
}

#[derive(Deserialize)]
struct RegisterData {
    server_id: String,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RegistrationOutcome {
    Confirmed {
        server_id: String,
    },
    /// The claim request may have committed but its response was not usable.
    /// The caller must try WebSocket authentication with the already-staged
    /// token before deciding whether to retry the same claim.
    Ambiguous,
}

#[derive(Debug)]
pub enum RegisterError {
    Persistence(String),
    PermanentAuth(String),
    RateLimited {
        retry_after: Duration,
        message: String,
    },
}

impl std::fmt::Display for RegisterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Persistence(message) => write!(f, "run-token persistence failed: {message}"),
            Self::PermanentAuth(message) => write!(f, "permanent auth failure: {message}"),
            Self::RateLimited {
                retry_after,
                message,
            } => write!(
                f,
                "rate limited (retry after {}s): {message}",
                retry_after.as_secs()
            ),
        }
    }
}

impl std::error::Error for RegisterError {}

/// Ensure the Agent owns a restart-safe run token before any claim request can
/// consume an Enrollment offer. Explicit `SERVERBEE_TOKEN` values are never
/// rewritten.
pub fn stage_run_token(config: &mut AgentConfig) -> Result<(), RegisterError> {
    stage_run_token_at(
        config,
        AgentConfig::config_path_for_persistence(),
        AgentConfig::token_env_override_present(),
    )
}

fn stage_run_token_at(
    config: &mut AgentConfig,
    path: impl AsRef<Path>,
    token_env_override_present: bool,
) -> Result<(), RegisterError> {
    if !config.token.is_empty() {
        return Ok(());
    }
    if token_env_override_present {
        return Err(RegisterError::Persistence(
            "SERVERBEE_TOKEN is present but empty; refusing to overwrite an explicit override"
                .to_string(),
        ));
    }

    let token = generate_run_token();
    crate::run_token_store::persist_run_token(path, &token)
        .map_err(|error| RegisterError::Persistence(error.to_string()))?;
    config.token = token;
    Ok(())
}

fn generate_run_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// One claim attempt. A run token is staged first, so persistence failure can
/// never burn the single-use Enrollment offer.
pub async fn register_agent(
    config: &mut AgentConfig,
) -> Result<RegistrationOutcome, RegisterError> {
    stage_run_token(config)?;
    register_once(config).await
}

async fn register_once(config: &AgentConfig) -> Result<RegistrationOutcome, RegisterError> {
    let url = format!(
        "{}/api/agent/register",
        config.server_url.trim_end_matches('/')
    );
    let response = match reqwest::Client::new()
        .post(url)
        .bearer_auth(&config.enrollment_code)
        .json(&RegisterRequest {
            proposed_run_token: &config.token,
        })
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!("Enrollment claim response is ambiguous: {error}");
            return Ok(RegistrationOutcome::Ambiguous);
        }
    };

    let status = response.status();
    if status.is_success() {
        let data: RegisterResponse = match response.json().await {
            Ok(data) => data,
            Err(error) => {
                tracing::warn!("Enrollment claim response body is ambiguous: {error}");
                return Ok(RegistrationOutcome::Ambiguous);
            }
        };
        tracing::info!(
            "Enrollment claim confirmed for server_id={}",
            data.data.server_id
        );
        return Ok(RegistrationOutcome::Confirmed {
            server_id: data.data.server_id,
        });
    }

    let retry_after = parse_retry_after(&response);
    let body = response.text().await.unwrap_or_default();
    let message = format!("HTTP {status}. Server said: {}", body.trim());
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            Err(RegisterError::PermanentAuth(message))
        }
        StatusCode::TOO_MANY_REQUESTS => Err(RegisterError::RateLimited {
            retry_after: retry_after.unwrap_or(DEFAULT_RATE_LIMIT_BACKOFF),
            message,
        }),
        _ => {
            tracing::warn!("Enrollment claim result is ambiguous: {message}");
            Ok(RegistrationOutcome::Ambiguous)
        }
    }
}

fn parse_retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

pub async fn register_agent_with_backoff(
    config: &mut AgentConfig,
) -> Result<RegistrationOutcome, RegisterError> {
    stage_run_token(config)?;
    for attempt in 1..=MAX_REGISTER_ATTEMPTS {
        match register_once(config).await {
            Ok(outcome) => return Ok(outcome),
            Err(RegisterError::RateLimited {
                retry_after,
                message,
            }) => {
                let wait = retry_after.min(MAX_BACKOFF);
                tracing::warn!(
                    "Enrollment claim rate-limited on attempt {attempt}/{MAX_REGISTER_ATTEMPTS}: \
                     {message}. Sleeping {}s before retry.",
                    wait.as_secs()
                );
                tokio::time::sleep(wait).await;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(RegistrationOutcome::Ambiguous)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    use super::*;
    use crate::config::{
        CapabilitiesConfig, CollectorConfig, FileConfig, IpChangeConfig, LogConfig, SecurityConfig,
        UpgradeConfig,
    };

    fn config(server_url: String) -> AgentConfig {
        AgentConfig {
            server_url,
            token: String::new(),
            enrollment_code: "enrollment-code-0123456789".to_string(),
            collector: CollectorConfig::default(),
            log: LogConfig::default(),
            file: FileConfig::default(),
            ip_change: IpChangeConfig::default(),
            upgrade: UpgradeConfig::default(),
            security: SecurityConfig::default(),
            capabilities: CapabilitiesConfig::default(),
        }
    }

    async fn spawn_server(
        response: &'static str,
    ) -> (
        String,
        oneshot::Receiver<String>,
        tokio::task::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("address");
        let (request_tx, request_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let read = socket.read(&mut buffer).await.expect("read");
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buffer[..read]);
                if let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n")
                {
                    let headers = String::from_utf8_lossy(&bytes[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .and_then(|value| value.trim().parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if bytes.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
            }
            let _ = request_tx.send(String::from_utf8_lossy(&bytes).into_owned());
            socket.write_all(response.as_bytes()).await.expect("write");
        });
        (format!("http://{address}"), request_rx, handle)
    }

    #[test]
    fn generated_run_tokens_are_base64url_unique_and_redaction_safe() {
        let first = generate_run_token();
        let second = generate_run_token();
        assert_eq!(first.len(), 43);
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        );
        assert_ne!(first, second);
    }

    #[test]
    fn staging_persists_before_exposing_token_to_the_caller() {
        let temp = TempDir::new().expect("tempdir");
        let path = temp.path().join("agent.toml");
        fs::write(&path, "server_url = \"http://127.0.0.1:9527\"\n").expect("seed");
        let mut config = config("http://127.0.0.1:9527".to_string());

        stage_run_token_at(&mut config, &path, false).expect("stage token");

        let persisted = fs::read_to_string(path).expect("read config");
        assert!(persisted.contains(&format!("token = \"{}\"", config.token)));
    }

    #[test]
    fn persistence_failure_leaves_no_in_memory_credential() {
        let temp = TempDir::new().expect("tempdir");
        let missing_parent = temp.path().join("missing").join("agent.toml");
        let mut config = config("http://127.0.0.1:9527".to_string());

        let result = stage_run_token_at(&mut config, missing_parent, false);

        assert!(matches!(result, Err(RegisterError::Persistence(_))));
        assert!(config.token.is_empty());
    }

    #[tokio::test]
    async fn claim_sends_staged_token() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 33\r\nConnection: close\r\n\r\n{\"data\":{\"server_id\":\"server-1\"}}";
        let (url, request_rx, handle) = spawn_server(response).await;
        let mut config = config(url);
        config.token = "staged-token-0123456789abcdefghijkl".to_string();

        let outcome = register_agent(&mut config).await.expect("claim");
        let request = request_rx.await.expect("request");
        handle.await.expect("server task");

        assert_eq!(
            outcome,
            RegistrationOutcome::Confirmed {
                server_id: "server-1".to_string()
            }
        );
        assert!(request.contains(&format!("\"proposed_run_token\":\"{}\"", config.token)));
    }

    #[tokio::test]
    async fn lost_success_body_is_ambiguous_and_keeps_staged_token() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
        let (url, _request_rx, handle) = spawn_server(response).await;
        let mut config = config(url);
        config.token = "staged-token-0123456789abcdefghijkl".to_string();

        let outcome = register_agent(&mut config).await.expect("ambiguous claim");
        handle.await.expect("server task");

        assert_eq!(outcome, RegistrationOutcome::Ambiguous);
        assert_eq!(config.token, "staged-token-0123456789abcdefghijkl");
    }
}
