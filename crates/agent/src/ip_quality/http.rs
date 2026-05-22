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
}
