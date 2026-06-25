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
    use std::sync::Arc;
    use std::time::Duration;

    use tempfile::TempDir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::{
        DEFAULT_RATE_LIMIT_BACKOFF, EXIT_CODE_PERMANENT_AUTH_FAILURE, RegisterError, register_agent,
        register_agent_with_backoff, register_once,
    };
    use crate::config::AgentConfig;

    /// A minimal local HTTP/1.1 mock. It accepts up to `responses.len()`
    /// connections (one request each) and replies with the queued raw
    /// response bytes in order, then keeps accepting and replying with the
    /// last response for any extra connections. Fully local (127.0.0.1),
    /// deterministic, no external network.
    async fn spawn_mock_server(responses: Vec<String>) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local_addr");
        let base_url = format!("http://{addr}");
        let responses = Arc::new(responses);

        let handle = tokio::spawn(async move {
            let mut idx = 0usize;
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                // Drain the request headers so the client's write completes.
                // We only need to consume enough to let the client proceed;
                // read once and ignore the contents.
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;

                let pick = idx.min(responses.len().saturating_sub(1));
                let body = responses
                    .get(pick)
                    .cloned()
                    .unwrap_or_else(|| http_response(200, "{}"));
                idx += 1;

                let _ = socket.write_all(body.as_bytes()).await;
                let _ = socket.flush().await;
                let _ = socket.shutdown().await;
            }
        });

        (base_url, handle)
    }

    /// Build a raw HTTP/1.1 response with a JSON body and a `Connection:
    /// close` so reqwest treats the body as complete on socket close.
    fn http_response(status: u16, body: &str) -> String {
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            403 => "Forbidden",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            _ => "Status",
        };
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// Same as [`http_response`] but injects an extra raw header line (e.g.
    /// `Retry-After: 0`).
    fn http_response_with_header(status: u16, header: &str, body: &str) -> String {
        let reason = match status {
            429 => "Too Many Requests",
            200 => "OK",
            _ => "Status",
        };
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\n{header}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn config_for(server_url: &str) -> AgentConfig {
        AgentConfig {
            server_url: server_url.to_string(),
            enrollment_code: "ENR-TEST".to_string(),
            ..Default::default()
        }
    }

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

    // ---- RegisterError::Display ----

    #[test]
    fn register_error_display_covers_all_variants() {
        let permanent = RegisterError::PermanentAuth("bad code".to_string());
        assert_eq!(
            permanent.to_string(),
            "permanent auth failure: bad code"
        );

        let rate_limited = RegisterError::RateLimited {
            retry_after: Duration::from_secs(42),
            message: "slow down".to_string(),
        };
        assert_eq!(
            rate_limited.to_string(),
            "rate limited (retry after 42s): slow down"
        );

        let transient = RegisterError::Transient("oops".to_string());
        assert_eq!(transient.to_string(), "transient error: oops");
    }

    #[test]
    fn register_error_implements_std_error() {
        // Exercise the std::error::Error blanket impl via trait object.
        let err: Box<dyn std::error::Error> =
            Box::new(RegisterError::Transient("x".to_string()));
        assert!(err.to_string().contains("transient error"));
        // Debug derive is present.
        let dbg = format!("{:?}", RegisterError::PermanentAuth("p".to_string()));
        assert!(dbg.contains("PermanentAuth"));
    }

    #[test]
    fn exit_code_constant_matches_systemd_contract() {
        assert_eq!(EXIT_CODE_PERMANENT_AUTH_FAILURE, 78);
    }

    // ---- register_once: happy path ----

    #[tokio::test]
    async fn register_once_returns_server_id_and_token_on_success() {
        let body = r#"{"data":{"server_id":"srv-123","token":"tok-abc"}}"#;
        let (url, handle) = spawn_mock_server(vec![http_response(200, body)]).await;
        let config = config_for(&url);

        let (server_id, token) = register_once(&config, "fp-1").await.expect("register ok");
        assert_eq!(server_id, "srv-123");
        assert_eq!(token, "tok-abc");

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_trims_trailing_slash_in_server_url() {
        let body = r#"{"data":{"server_id":"srv-9","token":"t9"}}"#;
        let (url, handle) = spawn_mock_server(vec![http_response(200, body)]).await;
        // Append a trailing slash; register_once must trim it before building
        // the /api/agent/register path.
        let config = config_for(&format!("{url}/"));

        let result = register_once(&config, "").await;
        assert!(result.is_ok(), "trailing slash should be trimmed: {result:?}");

        handle.abort();
    }

    // ---- register_once: invalid body ----

    #[tokio::test]
    async fn register_once_maps_invalid_body_to_transient() {
        // 200 OK but the body is not valid RegisterResponse JSON.
        let (url, handle) = spawn_mock_server(vec![http_response(200, "not json")]).await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::Transient(msg) => {
                assert!(
                    msg.contains("invalid response body"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected Transient, got {other:?}"),
        }

        handle.abort();
    }

    // ---- register_once: status-code categorization ----

    #[tokio::test]
    async fn register_once_maps_401_to_permanent_auth() {
        let (url, handle) =
            spawn_mock_server(vec![http_response(401, "invalid enrollment code")]).await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::PermanentAuth(msg) => {
                assert!(msg.contains("401"), "expected status in msg: {msg}");
                assert!(
                    msg.contains("invalid enrollment code"),
                    "expected server body in msg: {msg}"
                );
            }
            other => panic!("expected PermanentAuth, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_maps_403_to_permanent_auth() {
        let (url, handle) = spawn_mock_server(vec![http_response(403, "forbidden")]).await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        assert!(
            matches!(err, RegisterError::PermanentAuth(_)),
            "expected PermanentAuth, got {err:?}"
        );

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_maps_429_with_retry_after_header() {
        let (url, handle) = spawn_mock_server(vec![http_response_with_header(
            429,
            "Retry-After: 7",
            "slow down",
        )])
        .await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::RateLimited { retry_after, message } => {
                assert_eq!(retry_after, Duration::from_secs(7));
                assert!(message.contains("429"), "unexpected message: {message}");
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_429_without_retry_after_uses_default_backoff() {
        // No Retry-After header → parse_retry_after returns None → default.
        let (url, handle) = spawn_mock_server(vec![http_response(429, "throttled")]).await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, DEFAULT_RATE_LIMIT_BACKOFF);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_429_with_unparsable_retry_after_uses_default() {
        // Non-numeric Retry-After (HTTP-date form unsupported) → falls back.
        let (url, handle) = spawn_mock_server(vec![http_response_with_header(
            429,
            "Retry-After: Wed, 21 Oct 2015 07:28:00 GMT",
            "throttled",
        )])
        .await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::RateLimited { retry_after, .. } => {
                assert_eq!(retry_after, DEFAULT_RATE_LIMIT_BACKOFF);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }

        handle.abort();
    }

    #[tokio::test]
    async fn register_once_maps_500_to_transient() {
        let (url, handle) = spawn_mock_server(vec![http_response(500, "server boom")]).await;
        let config = config_for(&url);

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::Transient(msg) => {
                assert!(msg.contains("500"), "unexpected message: {msg}");
            }
            other => panic!("expected Transient, got {other:?}"),
        }

        handle.abort();
    }

    // ---- register_once: network error (no listener) ----

    #[tokio::test]
    async fn register_once_maps_connection_refused_to_transient() {
        // Bind then immediately drop the listener so the port is closed; the
        // POST should fail to connect and surface as a Transient network error.
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        drop(listener);
        let config = config_for(&format!("http://{addr}"));

        let err = register_once(&config, "fp").await.expect_err("should fail");
        match err {
            RegisterError::Transient(msg) => {
                assert!(msg.contains("network error"), "unexpected message: {msg}");
            }
            other => panic!("expected Transient, got {other:?}"),
        }
    }

    // ---- register_agent: wraps register_once errors ----

    #[tokio::test]
    async fn register_agent_wraps_error_with_guidance() {
        let (url, handle) = spawn_mock_server(vec![http_response(401, "nope")]).await;
        let config = config_for(&url);

        let err = register_agent(&config, "fp").await.expect_err("should fail");
        let msg = err.to_string();
        assert!(msg.contains("Registration failed"), "unexpected: {msg}");
        assert!(msg.contains("enrollment code"), "unexpected: {msg}");

        handle.abort();
    }

    #[tokio::test]
    async fn register_agent_returns_ok_tuple_on_success() {
        let body = r#"{"data":{"server_id":"S","token":"T"}}"#;
        let (url, handle) = spawn_mock_server(vec![http_response(200, body)]).await;
        let config = config_for(&url);

        let (sid, tok) = register_agent(&config, "fp").await.expect("ok");
        assert_eq!(sid, "S");
        assert_eq!(tok, "T");

        handle.abort();
    }

    // ---- register_agent_with_backoff ----

    #[tokio::test]
    async fn backoff_returns_immediately_on_success() {
        let body = r#"{"data":{"server_id":"sid","token":"tk"}}"#;
        let (url, handle) = spawn_mock_server(vec![http_response(200, body)]).await;
        let config = config_for(&url);

        let (sid, tk) = register_agent_with_backoff(&config, "fp")
            .await
            .expect("ok");
        assert_eq!(sid, "sid");
        assert_eq!(tk, "tk");

        handle.abort();
    }

    #[tokio::test]
    async fn backoff_returns_immediately_on_permanent_auth() {
        let (url, handle) = spawn_mock_server(vec![http_response(401, "bad")]).await;
        let config = config_for(&url);

        let err = register_agent_with_backoff(&config, "fp")
            .await
            .expect_err("should fail");
        assert!(
            matches!(err, RegisterError::PermanentAuth(_)),
            "expected PermanentAuth, got {err:?}"
        );

        handle.abort();
    }

    #[tokio::test]
    async fn backoff_honors_rate_limit_then_succeeds() {
        // First connection: 429 with Retry-After: 0 (zero-second sleep keeps
        // the test fast and deterministic). Second connection: 200 success.
        let body = r#"{"data":{"server_id":"after-throttle","token":"ttt"}}"#;
        let (url, handle) = spawn_mock_server(vec![
            http_response_with_header(429, "Retry-After: 0", "throttled"),
            http_response(200, body),
        ])
        .await;
        let config = config_for(&url);

        let (sid, tk) = register_agent_with_backoff(&config, "fp")
            .await
            .expect("ok after retry");
        assert_eq!(sid, "after-throttle");
        assert_eq!(tk, "ttt");

        handle.abort();
    }

    #[tokio::test]
    async fn backoff_retries_transient_then_succeeds() {
        // First connection: 500 (transient). Second connection: 200 success.
        // We pause tokio time so the 5s exponential backoff sleep resolves
        // instantly via auto-advance instead of stalling the test.
        tokio::time::pause();

        let body = r#"{"data":{"server_id":"recovered","token":"rk"}}"#;
        let (url, handle) = spawn_mock_server(vec![
            http_response(500, "boom"),
            http_response(200, body),
        ])
        .await;
        let config = config_for(&url);

        let (sid, tk) = register_agent_with_backoff(&config, "fp")
            .await
            .expect("ok after transient retry");
        assert_eq!(sid, "recovered");
        assert_eq!(tk, "rk");

        handle.abort();
    }
}
