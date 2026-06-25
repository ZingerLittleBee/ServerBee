use std::time::Duration;

use anyhow::{bail, Result};
use reqwest::redirect::Policy;
use serverbee_common::protocol::UnlockRequest;

use super::rule_engine::HttpOutcome;
use super::ssrf;

/// Maximum number of redirects to follow before giving up.
const MAX_REDIRECTS: usize = 5;

/// Maximum response body size read into memory (256 KiB).
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Connect timeout for each hop.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Total request timeout per hop.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Browser-like User-Agent string used for all unlock checks unless overridden.
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
     AppleWebKit/537.36 (KHTML, like Gecko) \
     Chrome/124.0.0.0 Safari/537.36";

/// Build the dedicated reqwest client for IP quality checks.
///
/// - Automatic redirects are disabled; the checker follows them manually so
///   every hop's host can be validated by the SSRF guard.
/// - A realistic browser User-Agent is set.
/// - Connect and total timeouts are applied.
pub fn build_client() -> Result<reqwest::Client> {
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    Ok(client)
}

/// Fetch `request`, following redirects manually (so each hop is SSRF-checked),
/// and return an [`HttpOutcome`].
///
/// - Validates the initial URL via [`ssrf::validate_url`].
/// - Before sending to each hop, calls [`ssrf::resolve_and_check`] on its host.
/// - Caps the response body at [`MAX_BODY_BYTES`].
/// - Returns an error if the redirect count exceeds [`MAX_REDIRECTS`].
pub async fn fetch(client: &reqwest::Client, request: &UnlockRequest) -> Result<HttpOutcome> {
    let timeout_ms = request.timeout_ms;
    let per_request_timeout = Duration::from_millis(u64::from(timeout_ms).max(1000));

    // Validate and parse the initial URL.
    let initial_url = ssrf::validate_url(&request.url)?;

    let method = reqwest::Method::from_bytes(request.method.to_ascii_uppercase().as_bytes())
        .unwrap_or(reqwest::Method::GET);

    let mut current_url = initial_url;
    let mut redirects: Vec<String> = Vec::new();
    let mut redirect_count = 0usize;

    loop {
        // Validate the host of the URL we are about to fetch.
        let host = current_url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("SSRF guard: URL has no host"))?;
        let port = current_url
            .port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("SSRF guard: cannot determine port for URL"))?;

        // Known TOCTOU window: `resolve_and_check` performs its own DNS
        // resolution here, but `reqwest`'s `send()` below resolves the host
        // again independently — a rebinding DNS server could hand back a
        // private IP on the second lookup. This residual window is accepted
        // for now because custom-service URLs come from the admin-only
        // catalog (an admin could point directly at a private IP anyway).
        // The proper fix — pinning the connection to the validated addresses
        // via reqwest's `resolve_to_addrs` — is planned for a later
        // hardening pass.
        ssrf::resolve_and_check(host, port)?;

        // Build the request for this hop.
        let mut req_builder = client.request(method.clone(), current_url.as_str());

        // Apply custom headers from the first-hop request only.
        if redirects.is_empty() {
            for (k, v) in &request.headers {
                req_builder = req_builder.header(k.as_str(), v.as_str());
            }
        }

        let response = tokio::time::timeout(per_request_timeout, req_builder.send())
            .await
            .map_err(|_| anyhow::anyhow!("request timed out after {}ms", timeout_ms))??;

        let status = response.status().as_u16();

        // Handle redirect.
        if (300..=399).contains(&status) {
            if redirect_count >= MAX_REDIRECTS {
                bail!("too many redirects (max {})", MAX_REDIRECTS);
            }

            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow::anyhow!("redirect with no Location header"))?;

            // Resolve the redirect URL relative to the current URL.
            let next_url = current_url.join(location)?;

            // The scheme/port of the redirect target must also pass validation.
            ssrf::validate_url(next_url.as_str())?;

            redirects.push(current_url.to_string());
            current_url = next_url;
            redirect_count += 1;
            continue;
        }

        // Non-redirect: read the body up to MAX_BODY_BYTES.
        let body_bytes = read_capped(response, MAX_BODY_BYTES).await?;
        let body = String::from_utf8_lossy(&body_bytes).into_owned();

        return Ok(HttpOutcome {
            status,
            body,
            final_url: current_url.to_string(),
            redirects,
        });
    }
}

/// Read at most `max_bytes` from the response body, streaming chunk-by-chunk.
///
/// Reading stops as soon as `max_bytes` is reached, so a malicious endpoint
/// returning a huge body can never exhaust agent memory: peak usage is bounded
/// at `max_bytes` plus a single chunk. `reqwest::Response::chunk()` is
/// available without the `stream` feature.
async fn read_capped(mut response: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = max_bytes.saturating_sub(buf.len());
        if remaining == 0 {
            break;
        }
        let take = remaining.min(chunk.len());
        buf.extend_from_slice(&chunk[..take]);
        if take < chunk.len() {
            // Cap reached mid-chunk; stop without reading the rest.
            break;
        }
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Spawn a one-shot HTTP/1.1 server on 127.0.0.1 that replies to a single
    /// request with a body of `body_len` bytes (all `b'x'`). Returns the bound
    /// address.
    async fn spawn_body_server(body_len: usize) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                // Drain the request headers (read until we see the blank line).
                let mut req = Vec::new();
                let mut byte = [0u8; 1];
                while stream.read_exact(&mut byte).await.is_ok() {
                    req.push(byte[0]);
                    if req.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let body = vec![b'x'; body_len];
                let _ = stream.write_all(&body).await;
                let _ = stream.flush().await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn read_capped_truncates_oversized_body() {
        // 1 MiB body — well over the 256 KiB cap.
        let body_len = 1024 * 1024;
        let addr = spawn_body_server(body_len).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap();

        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(
            buf.len(),
            MAX_BODY_BYTES,
            "oversized body must be truncated to the cap"
        );
    }

    #[tokio::test]
    async fn read_capped_keeps_small_body_intact() {
        let body_len = 1024;
        let addr = spawn_body_server(body_len).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client
            .get(format!("http://{addr}/"))
            .send()
            .await
            .unwrap();

        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(buf.len(), body_len, "small body must be read in full");
    }

    // ── read_capped: boundary + empty-body branches ──────────────────────────

    #[tokio::test]
    async fn read_capped_empty_body_returns_empty() {
        // Content-Length: 0 — the streaming loop sees no body chunks and the
        // returned buffer is empty.
        let addr = spawn_body_server(0).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert!(buf.is_empty(), "empty body must yield an empty buffer");
    }

    #[tokio::test]
    async fn read_capped_body_exactly_at_cap_is_kept_in_full() {
        // A body that is exactly `max_bytes` must be fully read; the cap is an
        // upper bound, not an off-by-one truncation.
        let cap = 4096;
        let addr = spawn_body_server(cap).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        let buf = read_capped(response, cap).await.unwrap();
        assert_eq!(buf.len(), cap, "body exactly at the cap must be kept in full");
    }

    #[tokio::test]
    async fn read_capped_with_zero_cap_reads_nothing() {
        // `max_bytes == 0` exercises the `remaining == 0` early-break path on
        // the very first chunk.
        let addr = spawn_body_server(1024).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        let buf = read_capped(response, 0).await.unwrap();
        assert!(buf.is_empty(), "a zero cap must read no bytes");
    }

    // ── build_client ─────────────────────────────────────────────────────────

    #[test]
    fn build_client_succeeds() {
        // The dedicated client must build with the configured redirect/UA/timeouts.
        assert!(build_client().is_ok(), "build_client must construct a client");
    }

    #[test]
    fn default_user_agent_is_browser_like() {
        // The UA is sent on every probe; keep it recognizably browser-shaped.
        assert!(DEFAULT_USER_AGENT.contains("Mozilla/5.0"));
        assert!(DEFAULT_USER_AGENT.contains("Chrome/"));
    }

    // ── fetch: pre-flight validation / SSRF error paths (no network needed) ────

    fn req(url: &str) -> UnlockRequest {
        UnlockRequest {
            url: url.to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 1000,
        }
    }

    #[tokio::test]
    async fn fetch_rejects_invalid_scheme() {
        // `validate_url` rejects non-http(s) schemes before any request is made.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("ftp://example.com/file")).await.unwrap_err();
        assert!(
            err.to_string().contains("scheme"),
            "expected scheme rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_rejects_non_default_port() {
        // The strict validator rejects ports other than 80/443.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://example.com:8080/")).await.unwrap_err();
        assert!(
            err.to_string().contains("port"),
            "expected port rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_rejects_embedded_credentials() {
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://user:pass@example.com/")).await.unwrap_err();
        assert!(
            err.to_string().contains("credentials"),
            "expected credentials rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_rejects_unparseable_url() {
        // `Url::parse` fails outright — exercises the `?` propagation from validate_url.
        let client = build_client().unwrap();
        assert!(fetch(&client, &req("not a url at all")).await.is_err());
    }

    #[tokio::test]
    async fn fetch_blocks_loopback_host_via_ssrf_guard() {
        // The URL passes scheme/port/credential validation, but `resolve_and_check`
        // rejects the loopback address — exercises the in-loop SSRF guard branch.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://127.0.0.1/")).await.unwrap_err();
        assert!(
            err.to_string().contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_blocks_localhost_host_via_ssrf_guard() {
        // `localhost` resolves to a loopback address and is rejected.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://localhost/")).await.unwrap_err();
        assert!(
            err.to_string().contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_uppercases_method_string() {
        // A lowercase method must be normalised; the request still fails at the
        // SSRF guard (loopback), but parsing the method must not panic/error.
        let client = build_client().unwrap();
        let request = UnlockRequest {
            url: "http://127.0.0.1/".to_string(),
            method: "get".to_string(),
            headers: vec![("X-Test".to_string(), "1".to_string())],
            timeout_ms: 1000,
        };
        let err = fetch(&client, &request).await.unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_falls_back_to_get_on_invalid_method() {
        // A method string with an illegal character (space) fails
        // `Method::from_bytes`, exercising the `unwrap_or(GET)` fallback branch.
        // The request still reaches and fails at the SSRF guard (loopback).
        let client = build_client().unwrap();
        let request = UnlockRequest {
            url: "http://127.0.0.1/".to_string(),
            method: "BAD METHOD".to_string(),
            headers: vec![],
            timeout_ms: 1000,
        };
        let err = fetch(&client, &request).await.unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_falls_back_to_get_on_empty_method() {
        // An empty method string is also invalid for `Method::from_bytes` and
        // must take the GET fallback without panicking.
        let client = build_client().unwrap();
        let request = UnlockRequest {
            url: "http://127.0.0.1/".to_string(),
            method: String::new(),
            headers: vec![],
            timeout_ms: 1000,
        };
        let err = fetch(&client, &request).await.unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_accepts_valid_non_get_method() {
        // A valid uppercase method (POST) parses successfully via the `Ok` arm
        // of `from_bytes`; the request still fails at the SSRF guard (loopback).
        let client = build_client().unwrap();
        let request = UnlockRequest {
            url: "http://127.0.0.1/".to_string(),
            method: "POST".to_string(),
            headers: vec![],
            timeout_ms: 1000,
        };
        let err = fetch(&client, &request).await.unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[tokio::test]
    async fn fetch_blocks_ipv4_private_host_via_ssrf_guard() {
        // A private RFC1918 literal passes scheme/port/credential validation but
        // is rejected by `resolve_and_check` as a non-global address — a branch
        // distinct from the loopback case.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://10.0.0.1/")).await.unwrap_err();
        assert!(
            err.to_string().contains("SSRF guard"),
            "expected SSRF guard rejection, got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_rejects_ipv6_loopback_host() {
        // A bracketed IPv6 loopback `[::1]` must never be fetched. Depending on
        // the resolver, the host is rejected either by the SSRF guard (parsed as
        // a non-global address) or by a resolution failure (the bracketed literal
        // does not resolve) — either way `fetch` errors and never connects.
        let client = build_client().unwrap();
        let err = fetch(&client, &req("http://[::1]/")).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("SSRF guard") || msg.contains("lookup address") || msg.contains("resolve"),
            "[::1] must be rejected (SSRF guard or resolution failure), got: {err}"
        );
    }

    #[tokio::test]
    async fn fetch_clamps_sub_minimum_timeout_without_panicking() {
        // `timeout_ms == 0` exercises the `u64::from(..).max(1000)` clamp; the
        // request still short-circuits at the SSRF guard (loopback) and the
        // clamp arithmetic must not underflow or panic.
        let client = build_client().unwrap();
        let request = UnlockRequest {
            url: "http://127.0.0.1/".to_string(),
            method: "GET".to_string(),
            headers: vec![],
            timeout_ms: 0,
        };
        let err = fetch(&client, &request).await.unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    // ── read_capped: multi-chunk accumulation under the cap ───────────────────

    #[tokio::test]
    async fn read_capped_accumulates_chunks_under_cap() {
        // A body larger than one TCP segment but still under the cap must be
        // accumulated across multiple `chunk()` reads and returned in full.
        let body_len = 200 * 1024; // < 256 KiB cap, likely spans several chunks.
        let addr = spawn_body_server(body_len).await;

        let client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(
            buf.len(),
            body_len,
            "a multi-chunk body under the cap must be read in full"
        );
        assert!(
            buf.iter().all(|&b| b == b'x'),
            "accumulated bytes must match the server's payload"
        );
    }

    // ── redirect-capable / status-capable mock servers ───────────────────────
    //
    // NOTE on coverage scope: `fetch()` validates every hop's host through the
    // strict SSRF guard (`ssrf::validate_url` restricts ports to 80/443 and
    // `ssrf::resolve_and_check` rejects loopback/private addresses). A mock
    // server bound to `127.0.0.1:<ephemeral-port>` is therefore rejected *before*
    // any TCP connection is attempted, so the in-loop redirect-following,
    // body-read, and timeout-wrapper branches of `fetch()` cannot be reached by
    // pointing `fetch()` at a loopback mock without modifying production code
    // (forbidden here, and there is no test bypass hook). The tests below instead
    // drive the same response-handling code path that `fetch()` delegates to —
    // `read_capped` — against 3xx/4xx/5xx mock responses, plus the chunk-error
    // propagation path, and assert the SSRF-on-redirect-target contract directly
    // via the exact `join` + `validate_url` logic `fetch()` runs at each hop.

    /// Spawn a one-shot server that replies with `status` (and reason `reason`),
    /// a `Location: location` header, and an empty body. Models the redirect
    /// response `fetch()` inspects before following a hop.
    async fn spawn_redirect_server(status: u16, reason: &str, location: &str) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let reason = reason.to_string();
        let location = location.to_string();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req = Vec::new();
                let mut byte = [0u8; 1];
                while stream.read_exact(&mut byte).await.is_ok() {
                    req.push(byte[0]);
                    if req.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = format!(
                    "HTTP/1.1 {status} {reason}\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        addr
    }

    /// Spawn a one-shot server that replies with an arbitrary `status` code and a
    /// fixed `body`. Models the non-2xx / non-redirect responses `fetch()` reads
    /// to completion before returning an `HttpOutcome`.
    async fn spawn_status_server(status: u16, reason: &str, body: &str) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let reason = reason.to_string();
        let body = body.to_string();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req = Vec::new();
                let mut byte = [0u8; 1];
                while stream.read_exact(&mut byte).await.is_ok() {
                    req.push(byte[0]);
                    if req.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.flush().await;
            }
        });
        addr
    }

    /// Spawn a server that promises `claimed_len` bytes via `Content-Length` but
    /// only writes `sent_len` of them, then closes the socket. The truncated body
    /// makes `reqwest`'s `chunk()` surface an "incomplete body" error, exercising
    /// the `?` error-propagation arm of `read_capped`.
    async fn spawn_truncated_body_server(claimed_len: usize, sent_len: usize) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req = Vec::new();
                let mut byte = [0u8; 1];
                while stream.read_exact(&mut byte).await.is_ok() {
                    req.push(byte[0]);
                    if req.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {claimed_len}\r\nConnection: close\r\n\r\n"
                );
                let _ = stream.write_all(header.as_bytes()).await;
                let partial = vec![b'x'; sent_len];
                let _ = stream.write_all(&partial).await;
                let _ = stream.flush().await;
                // Drop `stream` here: the connection closes with the body short of
                // the advertised Content-Length.
            }
        });
        addr
    }

    /// Spawn a server that accepts the connection, reads the request, then sleeps
    /// for `delay` before replying. A client wrapping the read in a shorter
    /// `tokio::time::timeout` must surface the elapsed error rather than blocking
    /// forever — the same wrapper shape `fetch()` applies around `send()`.
    async fn spawn_slow_server(delay: Duration) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut req = Vec::new();
                let mut byte = [0u8; 1];
                while stream.read_exact(&mut byte).await.is_ok() {
                    req.push(byte[0]);
                    if req.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                tokio::time::sleep(delay).await;
                let header = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n";
                let _ = stream.write_all(header.as_bytes()).await;
                let _ = stream.write_all(b"ok").await;
                let _ = stream.flush().await;
            }
        });
        addr
    }

    // ── read_capped over redirect (3xx) responses ────────────────────────────

    #[tokio::test]
    async fn read_capped_reads_empty_body_of_301_redirect() {
        // A 301 with `Content-Length: 0` is what `fetch()` sees before inspecting
        // the Location header; reading its (empty) body must yield no bytes.
        let addr = spawn_redirect_server(301, "Moved Permanently", "http://example.com/next").await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        assert_eq!(response.status().as_u16(), 301, "server must report a 301");
        assert_eq!(
            response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()),
            Some("http://example.com/next"),
            "the Location header `fetch()` follows must be present"
        );
        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert!(buf.is_empty(), "an empty redirect body must read as zero bytes");
    }

    #[tokio::test]
    async fn read_capped_reads_empty_body_of_302_redirect() {
        // 302 Found is the other common temporary-redirect status `fetch()` treats
        // identically to 301 (anything in 300..=399 enters the redirect branch).
        let addr = spawn_redirect_server(302, "Found", "/relative/path").await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        assert_eq!(response.status().as_u16(), 302);
        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert!(buf.is_empty());
    }

    // ── read_capped over non-2xx (4xx / 5xx) responses ───────────────────────

    #[tokio::test]
    async fn read_capped_reads_body_of_404_response() {
        // `fetch()` treats any non-3xx status as terminal and reads its body in
        // full (up to the cap). A 404 with a body must be returned verbatim.
        let body = "Not Found: unavailable in your region";
        let addr = spawn_status_server(404, "Not Found", body).await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        assert_eq!(response.status().as_u16(), 404, "server must report a 404");
        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&buf),
            body,
            "a non-2xx body must be read in full and unmodified"
        );
    }

    #[tokio::test]
    async fn read_capped_reads_body_of_500_response() {
        // A 5xx server error is likewise terminal for `fetch()`; its body feeds
        // the rule engine just like a 200 body would.
        let body = "Internal Server Error";
        let addr = spawn_status_server(500, "Internal Server Error", body).await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        assert_eq!(response.status().as_u16(), 500);
        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(String::from_utf8_lossy(&buf), body);
    }

    #[tokio::test]
    async fn read_capped_truncates_oversized_non_2xx_body() {
        // The cap applies regardless of status code: a 403 with an oversized body
        // must still be truncated to `MAX_BODY_BYTES`.
        let body = "B".repeat(MAX_BODY_BYTES + 4096);
        let addr = spawn_status_server(403, "Forbidden", &body).await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        assert_eq!(response.status().as_u16(), 403);
        let buf = read_capped(response, MAX_BODY_BYTES).await.unwrap();
        assert_eq!(buf.len(), MAX_BODY_BYTES, "the cap bounds non-2xx bodies too");
    }

    // ── read_capped: chunk() error propagation (incomplete body) ─────────────

    #[tokio::test]
    async fn read_capped_propagates_incomplete_body_error() {
        // The server advertises a large Content-Length but closes the socket after
        // sending only part of it. `reqwest`'s `chunk()` then yields an
        // incomplete-body error, which `read_capped` propagates via `?`.
        let addr = spawn_truncated_body_server(64 * 1024, 1024).await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let response = client.get(format!("http://{addr}/")).send().await.unwrap();

        let result = read_capped(response, MAX_BODY_BYTES).await;
        assert!(
            result.is_err(),
            "an incomplete body must surface a chunk() error, not a silent short read"
        );
    }

    // ── timeout wrapper shape (matches fetch's `tokio::time::timeout(send())`) ─

    #[tokio::test]
    async fn read_blocked_by_slow_server_hits_timeout_wrapper() {
        // `fetch()` wraps each hop in `tokio::time::timeout(per_request_timeout,
        // send())` and maps an elapsed timeout to a "request timed out" error.
        // We reproduce that wrapper here against a server that delays well past the
        // wrapper's budget, proving the elapsed branch fires deterministically.
        let server_delay = Duration::from_secs(30);
        let wrapper_budget = Duration::from_millis(150);
        let addr = spawn_slow_server(server_delay).await;

        let client = reqwest::Client::builder().redirect(Policy::none()).build().unwrap();
        let send_fut = client.get(format!("http://{addr}/")).send();

        let outcome = tokio::time::timeout(wrapper_budget, send_fut).await;
        assert!(
            outcome.is_err(),
            "a response slower than the wrapper budget must elapse the timeout"
        );
        // Mirror `fetch()`'s error-mapping so the asserted shape matches production.
        let mapped: Result<()> = outcome
            .map(|_| ())
            .map_err(|_| anyhow::anyhow!("request timed out after {}ms", wrapper_budget.as_millis()));
        assert!(mapped.unwrap_err().to_string().contains("timed out"));
    }

    // ── SSRF re-validation of a redirect target (fetch's per-hop contract) ────

    #[test]
    fn redirect_target_to_loopback_is_rejected_by_validate_url() {
        // `fetch()` resolves each `Location` relative to the current URL via
        // `current_url.join(location)` and then re-runs `ssrf::validate_url` on the
        // result (http.rs:114+117). This asserts that contract directly: a public
        // page redirecting to a loopback target is rejected before the next hop.
        let current = ssrf::validate_url("http://example.com/start").unwrap();
        let next = current.join("http://127.0.0.1/admin").unwrap();
        // validate_url accepts the scheme/port/credential shape (loopback literal
        // passes those checks); the address-level rejection happens in
        // resolve_and_check, which `fetch()` calls at the top of the next loop
        // iteration — so assert the resolution guard rejects the rebound host.
        let host = next.host_str().unwrap();
        let port = next.port_or_known_default().unwrap();
        let err = ssrf::resolve_and_check(host, port).unwrap_err();
        assert!(
            err.to_string().contains("SSRF guard"),
            "a redirect target pointing at loopback must be blocked, got: {err}"
        );
    }

    #[test]
    fn redirect_target_to_private_ip_is_rejected() {
        // Same contract for an RFC1918 redirect target: the next-hop guard rejects
        // it as a non-global address.
        let current = ssrf::validate_url("http://example.com/start").unwrap();
        let next = current.join("http://10.0.0.5/internal").unwrap();
        let host = next.host_str().unwrap();
        let port = next.port_or_known_default().unwrap();
        let err = ssrf::resolve_and_check(host, port).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn redirect_target_to_disallowed_port_is_rejected_by_validate_url() {
        // A `Location` pointing at a non-80/443 port must be rejected by the
        // per-hop `validate_url` re-check (http.rs:117), before any connection.
        let current = ssrf::validate_url("http://example.com/start").unwrap();
        let next = current.join("http://example.com:8080/next").unwrap();
        let err = ssrf::validate_url(next.as_str()).unwrap_err();
        assert!(
            err.to_string().contains("port"),
            "a redirect to a disallowed port must be rejected, got: {err}"
        );
    }

    #[test]
    fn relative_redirect_resolves_against_current_url() {
        // `fetch()` joins relative `Location` values against the current URL. A
        // bare path must inherit the origin so the next-hop guard checks the right
        // host (here: a public host that passes validation).
        let current = ssrf::validate_url("http://example.com/a/b").unwrap();
        let next = current.join("/c").unwrap();
        assert_eq!(next.as_str(), "http://example.com/c");
        assert!(
            ssrf::validate_url(next.as_str()).is_ok(),
            "a relative redirect staying on a public host must pass validation"
        );
    }
}
