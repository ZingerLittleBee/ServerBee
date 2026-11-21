//! Host `tailscaled` LocalAPI client and `ipnstate.Status` JSON parser.
//!
//! Queries `GET /localapi/v0/status` (`Host: local-tailscaled.sock`) over a
//! Unix domain socket on Unix platforms, falling back to `tailscale status --json`
//! when the socket is missing or inaccessible, or on non-Unix platforms.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Deserializer};
use serverbee_common::tailscale_types::{
    TailscaleNodeSummary, TailscalePeerInfo, TailscaleStatusReport,
};

use crate::config::TailscaleConfig;

/// Default `tailscaled` Unix socket locations probed in order when
/// `tailscale.socket_path` is empty.
#[cfg(unix)]
pub const DEFAULT_TAILSCALE_SOCKETS: &[&str] = &[
    "/var/run/tailscale/tailscaled.sock",
    "/run/tailscale/tailscaled.sock",
    "/var/run/tailscaled.socket",
];

const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

#[cfg(unix)]
const MAX_LOCALAPI_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

#[async_trait]
pub trait TailscaleProbe: Send + Sync {
    async fn fetch_status(&self) -> anyhow::Result<TailscaleStatusReport>;
}

/// Production probe trying `tailscaled` Unix socket(s) first, then the
/// `tailscale status --json` CLI subprocess.
pub struct SystemTailscaleProbe {
    socket_path: String,
    cli_program: String,
    #[cfg(test)]
    disable_cli_fallback: bool,
}

impl SystemTailscaleProbe {
    pub fn from_config(config: &TailscaleConfig) -> Self {
        Self {
            socket_path: config.socket_path.clone(),
            cli_program: "tailscale".to_string(),
            #[cfg(test)]
            disable_cli_fallback: config.disable_cli_fallback_for_test,
        }
    }

    #[cfg(test)]
    pub fn with_paths_for_test(
        socket_path: impl Into<String>,
        cli_program: impl Into<String>,
    ) -> Self {
        Self {
            socket_path: socket_path.into(),
            cli_program: cli_program.into(),
            disable_cli_fallback: false,
        }
    }

    #[cfg(unix)]
    fn candidate_sockets(&self) -> Vec<PathBuf> {
        let trimmed = self.socket_path.trim();
        if !trimmed.is_empty() {
            vec![PathBuf::from(trimmed)]
        } else {
            DEFAULT_TAILSCALE_SOCKETS
                .iter()
                .map(PathBuf::from)
                .collect()
        }
    }
}

#[async_trait]
impl TailscaleProbe for SystemTailscaleProbe {
    async fn fetch_status(&self) -> anyhow::Result<TailscaleStatusReport> {
        let mut last_socket_err: Option<anyhow::Error> = None;

        #[cfg(unix)]
        {
            for sock in self.candidate_sockets() {
                match query_localapi_unix(&sock).await {
                    Ok(body) => return parse_tailscale_status(&body),
                    Err(err) => {
                        tracing::debug!(
                            socket = %sock.display(),
                            error = %err,
                            "tailscaled LocalAPI socket probe failed"
                        );
                        last_socket_err = Some(err);
                    }
                }
            }
        }

        #[cfg(test)]
        if self.disable_cli_fallback {
            return Err(last_socket_err
                .unwrap_or_else(|| anyhow::anyhow!("tailscale CLI fallback disabled for test")));
        }

        match query_cli_status(&self.cli_program).await {
            Ok(body) => parse_tailscale_status(&body),
            Err(cli_err) => {
                if let Some(sock_err) = last_socket_err {
                    anyhow::bail!("tailscaled unavailable (socket: {sock_err}; cli: {cli_err})");
                }
                Err(cli_err)
            }
        }
    }
}

/// Issue `GET /localapi/v0/status` (`Host: local-tailscaled.sock`) over a Unix
/// domain socket with a 3s timeout.
#[cfg(unix)]
pub async fn query_localapi_unix(socket_path: &Path) -> anyhow::Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    tokio::time::timeout(PROBE_TIMEOUT, async move {
        let mut stream = tokio::net::UnixStream::connect(socket_path).await?;
        let request = concat!(
            "GET /localapi/v0/status HTTP/1.1\r\n",
            "Host: local-tailscaled.sock\r\n",
            "Accept: application/json\r\n",
            "Connection: close\r\n",
            "\r\n"
        );
        stream.write_all(request.as_bytes()).await?;

        let mut raw = Vec::new();
        (&mut stream)
            .take((MAX_LOCALAPI_RESPONSE_BYTES + 1) as u64)
            .read_to_end(&mut raw)
            .await?;
        if raw.len() > MAX_LOCALAPI_RESPONSE_BYTES {
            anyhow::bail!(
                "tailscaled LocalAPI response exceeded {MAX_LOCALAPI_RESPONSE_BYTES} bytes"
            );
        }
        parse_http_response(&raw)
    })
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "tailscaled socket {} timed out after {}s",
            socket_path.display(),
            PROBE_TIMEOUT.as_secs()
        )
    })?
}

#[cfg(unix)]
fn parse_http_response(raw: &[u8]) -> anyhow::Result<String> {
    let (header_bytes, body_bytes) =
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            (&raw[..pos], &raw[pos + 4..])
        } else if let Some(pos) = raw.windows(2).position(|w| w == b"\n\n") {
            (&raw[..pos], &raw[pos + 2..])
        } else {
            anyhow::bail!("malformed HTTP response from tailscaled: missing header terminator");
        };

    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| anyhow::anyhow!("non-UTF-8 HTTP headers from tailscaled"))?;
    let mut lines = header_str.lines();
    let status_line = lines
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty HTTP response from tailscaled"))?;
    let status_code: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP status line: {status_line}"))?;

    if status_code != 200 {
        anyhow::bail!("tailscaled LocalAPI returned HTTP {status_code}");
    }

    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name_trimmed = name.trim();
            let val_trimmed = value.trim();
            if name_trimmed.eq_ignore_ascii_case("transfer-encoding")
                && val_trimmed.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            } else if name_trimmed.eq_ignore_ascii_case("content-length")
                && let Ok(len) = val_trimmed.parse::<usize>()
            {
                content_length = Some(len);
            }
        }
    }

    let decoded = if is_chunked {
        decode_chunked_body(body_bytes)?
    } else if let Some(len) = content_length {
        if body_bytes.len() < len {
            anyhow::bail!(
                "truncated HTTP body from tailscaled (got {} bytes, expected {len})",
                body_bytes.len()
            );
        }
        body_bytes[..len].to_vec()
    } else {
        body_bytes.to_vec()
    };

    String::from_utf8(decoded)
        .map_err(|_| anyhow::anyhow!("tailscaled response body is not valid UTF-8"))
}

#[cfg(unix)]
fn decode_chunked_body(mut input: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        let line_end = input
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| anyhow::anyhow!("malformed chunk header in tailscaled response"))?;
        let header = std::str::from_utf8(&input[..line_end])
            .map_err(|_| anyhow::anyhow!("non-UTF-8 chunk size in tailscaled response"))?;
        let size_hex = header.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(size_hex, 16)
            .map_err(|_| anyhow::anyhow!("invalid chunk size '{size_hex}'"))?;
        input = &input[line_end + 2..];
        if chunk_size == 0 {
            break;
        }
        if input.len() < chunk_size + 2 || &input[chunk_size..chunk_size + 2] != b"\r\n" {
            anyhow::bail!("truncated chunk payload in tailscaled response");
        }
        out.extend_from_slice(&input[..chunk_size]);
        input = &input[chunk_size + 2..];
    }
    Ok(out)
}

async fn query_cli_status(program: &str) -> anyhow::Result<String> {
    let fut = tokio::process::Command::new(program)
        .args(["status", "--json"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .output();

    let output = tokio::time::timeout(PROBE_TIMEOUT, fut)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "{program} status --json timed out after {}s",
                PROBE_TIMEOUT.as_secs()
            )
        })??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "{program} status --json exited with {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let stdout = String::from_utf8(output.stdout)?;
    if stdout.trim().is_empty() {
        anyhow::bail!("{program} status --json returned empty output");
    }
    Ok(stdout)
}

// ---------------------------------------------------------------------------
// Go `ipnstate.Status` JSON deserialization & normalization
// ---------------------------------------------------------------------------

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct RawTailscaleStatus {
    #[serde(rename = "BackendState", default)]
    backend_state: Option<String>,
    #[serde(rename = "Version", default)]
    version: Option<String>,
    #[serde(rename = "TailscaleIPs", default, deserialize_with = "null_to_default")]
    tailscale_ips: Vec<String>,
    #[serde(rename = "MagicDNSSuffix", default)]
    magic_dns_suffix: Option<String>,
    #[serde(rename = "CurrentTailnet", default)]
    current_tailnet: Option<RawTailnetStatus>,
    #[serde(rename = "Self", default)]
    self_node: Option<RawPeerStatus>,
    #[serde(rename = "Peer", default, deserialize_with = "null_to_default")]
    peers: BTreeMap<String, RawPeerStatus>,
    #[serde(rename = "Health", default, deserialize_with = "null_to_default")]
    health: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawTailnetStatus {
    #[serde(rename = "Name", default)]
    name: Option<String>,
    #[serde(rename = "MagicDNSSuffix", default)]
    magic_dns_suffix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawPeerStatus {
    #[serde(rename = "ID", default)]
    id: Option<serde_json::Value>,
    #[serde(rename = "HostName", default)]
    host_name: Option<String>,
    #[serde(rename = "DNSName", default)]
    dns_name: Option<String>,
    #[serde(rename = "OS", default)]
    os: Option<String>,
    #[serde(rename = "TailscaleIPs", default, deserialize_with = "null_to_default")]
    tailscale_ips: Vec<String>,
    #[serde(rename = "Online", default)]
    online: Option<bool>,
    #[serde(rename = "Active", default)]
    active: Option<bool>,
    #[serde(rename = "CurAddr", default)]
    cur_addr: Option<String>,
    #[serde(rename = "Relay", default)]
    relay: Option<String>,
    #[serde(rename = "PeerRelay", default)]
    peer_relay: Option<String>,
    #[serde(rename = "RxBytes", default)]
    rx_bytes: Option<i64>,
    #[serde(rename = "TxBytes", default)]
    tx_bytes: Option<i64>,
    #[serde(rename = "LastHandshake", default)]
    last_handshake: Option<String>,
    #[serde(rename = "LastSeen", default)]
    last_seen: Option<String>,
    #[serde(rename = "ExitNode", default)]
    exit_node: Option<bool>,
    #[serde(rename = "ExitNodeOption", default)]
    exit_node_option: Option<bool>,
    #[serde(rename = "KeyExpiry", default)]
    key_expiry: Option<String>,
    #[serde(
        rename = "PrimaryRoutes",
        default,
        deserialize_with = "null_to_default"
    )]
    primary_routes: Vec<String>,
    #[serde(rename = "AllowedIPs", default, deserialize_with = "null_to_default")]
    allowed_ips: Vec<String>,
    #[serde(rename = "Tags", default, deserialize_with = "null_to_default")]
    tags: Vec<String>,
}

fn non_empty_trimmed(val: Option<String>) -> Option<String> {
    let trimmed = val?.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_dns_name(val: Option<String>) -> Option<String> {
    let trimmed = val?.trim().trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Go's zero `time.Time` serializes as `"0001-01-01T00:00:00Z"`; treat it as absent.
fn normalize_go_timestamp(val: Option<String>) -> Option<String> {
    let ts = non_empty_trimmed(val)?;
    if ts.starts_with("0001-01-01") {
        None
    } else {
        Some(ts)
    }
}

fn value_to_id_string(val: Option<serde_json::Value>, fallback_key: &str) -> String {
    match val {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => fallback_key.to_string(),
    }
}

fn classify_connection_mode(
    cur_addr: Option<&str>,
    peer_relay: Option<&str>,
    relay: Option<&str>,
    active: bool,
) -> &'static str {
    if cur_addr.is_some_and(|s| !s.is_empty()) {
        "direct"
    } else if peer_relay.is_some_and(|s| !s.is_empty()) {
        "peer_relay"
    } else if relay.is_some_and(|s| !s.is_empty()) && active {
        "derp"
    } else {
        "idle"
    }
}

/// Parse raw `tailscaled` `Status` JSON into [`TailscaleStatusReport`].
pub fn parse_tailscale_status(json_str: &str) -> anyhow::Result<TailscaleStatusReport> {
    parse_tailscale_status_at(json_str, Utc::now().to_rfc3339())
}

fn parse_tailscale_status_at(
    json_str: &str,
    collected_at: String,
) -> anyhow::Result<TailscaleStatusReport> {
    let raw: RawTailscaleStatus = serde_json::from_str(json_str)?;

    let backend_state =
        non_empty_trimmed(raw.backend_state).unwrap_or_else(|| "Unknown".to_string());
    let version = non_empty_trimmed(raw.version);

    let tailnet_name = raw
        .current_tailnet
        .as_ref()
        .and_then(|ct| {
            non_empty_trimmed(ct.name.clone()).or_else(|| {
                ct.magic_dns_suffix
                    .clone()
                    .and_then(|s| normalize_dns_name(Some(s)))
            })
        })
        .or_else(|| normalize_dns_name(raw.magic_dns_suffix));

    let self_node = raw.self_node;
    let tailscale_ips = if !raw.tailscale_ips.is_empty() {
        raw.tailscale_ips
    } else {
        self_node
            .as_ref()
            .map(|s| s.tailscale_ips.clone())
            .unwrap_or_default()
    };

    let (
        dns_name,
        hostname,
        online,
        exit_node,
        exit_node_option,
        key_expiry,
        advertised_routes,
        tags,
        derp_region,
    ) = if let Some(me) = self_node {
        let dns = normalize_dns_name(me.dns_name);
        let host = non_empty_trimmed(me.host_name);
        let is_online = me.online.unwrap_or(backend_state == "Running");
        let is_exit = me.exit_node.unwrap_or(false);
        let is_exit_opt = me.exit_node_option.unwrap_or(false);
        let expiry = normalize_go_timestamp(me.key_expiry);
        let relay = non_empty_trimmed(me.relay);

        // Build advertised subnet routes from PrimaryRoutes + AllowedIPs while
        // excluding the node's own single-host Tailscale /32 and /128 addresses.
        let own_host_cidrs: BTreeSet<String> = tailscale_ips
            .iter()
            .flat_map(|ip| [format!("{ip}/32"), format!("{ip}/128"), ip.clone()])
            .collect();
        let mut routes_seen = BTreeSet::new();
        let mut routes = Vec::new();
        for route in me.primary_routes.into_iter().chain(me.allowed_ips) {
            let trimmed = route.trim().to_string();
            if !trimmed.is_empty()
                && !own_host_cidrs.contains(&trimmed)
                && routes_seen.insert(trimmed.clone())
            {
                routes.push(trimmed);
            }
        }

        (
            dns,
            host,
            is_online,
            is_exit,
            is_exit_opt,
            expiry,
            routes,
            me.tags,
            relay,
        )
    } else {
        (
            None,
            None,
            backend_state == "Running",
            false,
            false,
            None,
            Vec::new(),
            Vec::new(),
            None,
        )
    };

    let mut peers: Vec<TailscalePeerInfo> = raw
        .peers
        .into_iter()
        .map(|(peer_key, p)| {
            let id = value_to_id_string(p.id, &peer_key);
            let host_name = p.host_name.unwrap_or_default().trim().to_string();
            let dns_name = normalize_dns_name(p.dns_name);
            let os = non_empty_trimmed(p.os);
            let online = p.online.unwrap_or(false);
            let active = p.active.unwrap_or(false);
            let cur_addr = non_empty_trimmed(p.cur_addr);
            let relay = non_empty_trimmed(p.relay);
            let peer_relay = non_empty_trimmed(p.peer_relay);
            let connection_mode = classify_connection_mode(
                cur_addr.as_deref(),
                peer_relay.as_deref(),
                relay.as_deref(),
                active,
            )
            .to_string();

            TailscalePeerInfo {
                id,
                host_name,
                dns_name,
                os,
                tailscale_ips: p.tailscale_ips,
                online,
                active,
                connection_mode,
                cur_addr,
                relay,
                rx_bytes: p.rx_bytes.unwrap_or(0).max(0) as u64,
                tx_bytes: p.tx_bytes.unwrap_or(0).max(0) as u64,
                last_handshake: normalize_go_timestamp(p.last_handshake),
                last_seen: normalize_go_timestamp(p.last_seen),
                exit_node: p.exit_node.unwrap_or(false),
                exit_node_option: p.exit_node_option.unwrap_or(false),
                tags: p.tags,
            }
        })
        .collect();

    // Deterministic order: online peers first, then alphabetical by host_name, then id.
    peers.sort_by(|a, b| {
        b.online
            .cmp(&a.online)
            .then_with(|| a.host_name.cmp(&b.host_name))
            .then_with(|| a.id.cmp(&b.id))
    });

    let health_warnings = raw
        .health
        .into_iter()
        .filter_map(|w| non_empty_trimmed(Some(w)))
        .collect();

    Ok(TailscaleStatusReport {
        summary: TailscaleNodeSummary {
            backend_state,
            tailscale_ips,
            dns_name,
            tailnet_name,
            version,
            hostname,
            online,
            exit_node,
            exit_node_option,
            key_expiry,
        },
        advertised_routes,
        tags,
        derp_region,
        peers,
        health_warnings,
        collected_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_STATUS_JSON: &str = r#"{
        "Version": "1.78.1-t12345",
        "BackendState": "Running",
        "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::103"],
        "MagicDNSSuffix": "tail99.ts.net.",
        "CurrentTailnet": {
            "Name": "example-org.github",
            "MagicDNSSuffix": "tail99.ts.net",
            "MagicDNSEnabled": true
        },
        "Health": ["Some peers are using DERP"],
        "Self": {
            "ID": "nSelf01",
            "HostName": "prod-db-1",
            "DNSName": "prod-db-1.tail99.ts.net.",
            "OS": "linux",
            "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::103"],
            "Online": true,
            "ExitNode": false,
            "ExitNodeOption": true,
            "KeyExpiry": "2027-05-01T12:00:00Z",
            "PrimaryRoutes": ["10.20.0.0/16"],
            "AllowedIPs": ["100.101.102.103/32", "fd7a:115c:a1e0::103/128", "10.20.0.0/16", "192.168.50.0/24"],
            "Tags": ["tag:server", "tag:db"],
            "Relay": "tok"
        },
        "Peer": {
            "nodekey:direct1": {
                "ID": "nPeerDirect",
                "HostName": "edge-gw",
                "DNSName": "edge-gw.tail99.ts.net.",
                "OS": "linux",
                "TailscaleIPs": ["100.101.102.10"],
                "Online": true,
                "Active": true,
                "CurAddr": "198.51.100.8:41641",
                "Relay": "sfo",
                "RxBytes": 4096,
                "TxBytes": 8192,
                "LastHandshake": "2026-11-21T03:30:00Z",
                "LastSeen": "0001-01-01T00:00:00Z",
                "ExitNode": true,
                "ExitNodeOption": true,
                "Tags": ["tag:edge"]
            },
            "nodekey:derp2": {
                "ID": "nPeerDerp",
                "HostName": "nat-worker",
                "DNSName": "nat-worker.tail99.ts.net.",
                "OS": "linux",
                "TailscaleIPs": ["100.101.102.11"],
                "Online": true,
                "Active": true,
                "CurAddr": "",
                "Relay": "sin",
                "RxBytes": 512,
                "TxBytes": 1024,
                "LastHandshake": "2026-11-21T03:29:00Z",
                "LastSeen": "0001-01-01T00:00:00Z"
            },
            "nodekey:peerrelay3": {
                "ID": "nPeerRelay",
                "HostName": "relay-client",
                "DNSName": "relay-client.tail99.ts.net.",
                "OS": "macOS",
                "TailscaleIPs": ["100.101.102.12"],
                "Online": true,
                "Active": true,
                "CurAddr": "",
                "PeerRelay": "198.51.100.99:40000",
                "Relay": "tok",
                "RxBytes": 100,
                "TxBytes": 200
            },
            "nodekey:idle4": {
                "ID": "nPeerIdle",
                "HostName": "laptop",
                "DNSName": "laptop.tail99.ts.net.",
                "OS": "iOS",
                "TailscaleIPs": ["100.101.102.13"],
                "Online": false,
                "Active": false,
                "CurAddr": "",
                "Relay": "lax",
                "LastHandshake": "0001-01-01T00:00:00Z",
                "LastSeen": "2026-11-20T22:00:00Z"
            }
        }
    }"#;

    #[test]
    fn parses_full_tailscaled_status_and_classifies_all_peer_modes() {
        let report =
            parse_tailscale_status_at(SAMPLE_STATUS_JSON, "2026-11-21T03:35:00Z".into()).unwrap();

        assert_eq!(report.summary.backend_state, "Running");
        assert_eq!(
            report.summary.tailscale_ips,
            vec!["100.101.102.103", "fd7a:115c:a1e0::103"]
        );
        assert_eq!(
            report.summary.dns_name.as_deref(),
            Some("prod-db-1.tail99.ts.net"),
            "trailing dot on DNSName must be trimmed"
        );
        assert_eq!(
            report.summary.tailnet_name.as_deref(),
            Some("example-org.github")
        );
        assert_eq!(report.summary.version.as_deref(), Some("1.78.1-t12345"));
        assert_eq!(report.summary.hostname.as_deref(), Some("prod-db-1"));
        assert!(report.summary.online);
        assert!(!report.summary.exit_node);
        assert!(report.summary.exit_node_option);
        assert_eq!(
            report.summary.key_expiry.as_deref(),
            Some("2027-05-01T12:00:00Z")
        );
        assert_eq!(
            report.advertised_routes,
            vec!["10.20.0.0/16", "192.168.50.0/24"],
            "own /32 and /128 TailscaleIPs must be excluded from advertised_routes"
        );
        assert_eq!(report.tags, vec!["tag:server", "tag:db"]);
        assert_eq!(report.derp_region.as_deref(), Some("tok"));
        assert_eq!(report.health_warnings, vec!["Some peers are using DERP"]);
        assert_eq!(report.collected_at, "2026-11-21T03:35:00Z");

        assert_eq!(report.peers.len(), 4);
        let by_id: BTreeMap<&str, &TailscalePeerInfo> =
            report.peers.iter().map(|p| (p.id.as_str(), p)).collect();

        let direct = by_id["nPeerDirect"];
        assert_eq!(direct.connection_mode, "direct");
        assert_eq!(direct.cur_addr.as_deref(), Some("198.51.100.8:41641"));
        assert_eq!(direct.dns_name.as_deref(), Some("edge-gw.tail99.ts.net"));
        assert_eq!(direct.rx_bytes, 4096);
        assert_eq!(direct.tx_bytes, 8192);
        assert!(direct.last_seen.is_none(), "Go zero time must become None");
        assert!(direct.exit_node);

        let derp = by_id["nPeerDerp"];
        assert_eq!(derp.connection_mode, "derp");
        assert!(derp.cur_addr.is_none());
        assert_eq!(derp.relay.as_deref(), Some("sin"));

        let peer_relay = by_id["nPeerRelay"];
        assert_eq!(peer_relay.connection_mode, "peer_relay");

        let idle = by_id["nPeerIdle"];
        assert_eq!(idle.connection_mode, "idle");
        assert!(!idle.online);
        assert!(idle.last_handshake.is_none());
        assert_eq!(idle.last_seen.as_deref(), Some("2026-11-20T22:00:00Z"));
    }

    #[test]
    fn handles_go_null_slices_and_stopped_state_gracefully() {
        let json = r#"{
            "BackendState": "Stopped",
            "TailscaleIPs": null,
            "MagicDNSSuffix": "corp.ts.net.",
            "CurrentTailnet": null,
            "Health": null,
            "Self": {
                "ID": 42,
                "HostName": "stopped-box",
                "DNSName": "",
                "TailscaleIPs": null,
                "Online": false,
                "KeyExpiry": "0001-01-01T00:00:00Z",
                "PrimaryRoutes": null,
                "AllowedIPs": null,
                "Tags": null
            },
            "Peer": null
        }"#;

        let report = parse_tailscale_status_at(json, "2026-11-21T03:35:00Z".into()).unwrap();
        assert_eq!(report.summary.backend_state, "Stopped");
        assert!(!report.summary.online);
        assert!(report.summary.tailscale_ips.is_empty());
        assert!(report.summary.dns_name.is_none());
        assert_eq!(report.summary.tailnet_name.as_deref(), Some("corp.ts.net"));
        assert!(report.summary.key_expiry.is_none());
        assert!(report.peers.is_empty());
        assert!(report.health_warnings.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn localapi_unix_socket_handles_content_length_and_verifies_host_header() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("tailscaled.sock");
        let listener = tokio::net::UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut req_buf = [0u8; 1024];
            let n = stream.read(&mut req_buf).await.unwrap();
            let req = String::from_utf8_lossy(&req_buf[..n]).to_string();
            assert!(req.starts_with("GET /localapi/v0/status HTTP/1.1\r\n"));
            assert!(
                req.to_ascii_lowercase()
                    .contains("host: local-tailscaled.sock\r\n")
            );

            let body = r#"{"BackendState":"Running","TailscaleIPs":["100.64.0.9"]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        });

        let probe =
            SystemTailscaleProbe::with_paths_for_test(sock_path.to_string_lossy(), "/nonexistent");
        let status = probe.fetch_status().await.unwrap();
        server.await.unwrap();

        assert_eq!(status.summary.backend_state, "Running");
        assert_eq!(status.summary.tailscale_ips, vec!["100.64.0.9"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn localapi_unix_socket_decodes_chunked_transfer_encoding() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("chunked.sock");
        let listener = tokio::net::UnixListener::bind(&sock_path).unwrap();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut req_buf = [0u8; 512];
            let _ = stream.read(&mut req_buf).await.unwrap();

            let part1 = br#"{"BackendState":"Running","#;
            let part2 = br#""TailscaleIPs":["100.64.1.2"]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n{:x}\r\n{}\r\n0\r\n\r\n",
                part1.len(),
                std::str::from_utf8(part1).unwrap(),
                part2.len(),
                std::str::from_utf8(part2).unwrap()
            );
            stream.write_all(resp.as_bytes()).await.unwrap();
        });

        let body = query_localapi_unix(&sock_path).await.unwrap();
        server.await.unwrap();
        let report = parse_tailscale_status(&body).unwrap();
        assert_eq!(report.summary.tailscale_ips, vec!["100.64.1.2"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn falls_back_to_cli_when_socket_returns_permission_denied_or_missing() {
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let dir = tempfile::tempdir().unwrap();
        let sock_path = dir.path().join("forbidden.sock");
        let listener = tokio::net::UnixListener::bind(&sock_path).unwrap();

        // Mock socket returning HTTP 403 Forbidden (simulating unprivileged LocalAPI denial).
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 512];
            let _ = stream.read(&mut buf).await;
            let _ = stream
                .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 6\r\n\r\ndenied")
                .await;
        });

        // Create a mock CLI script that validates `status --json` args and prints JSON.
        let cli_path = dir.path().join("mock-tailscale.sh");
        std::fs::write(
            &cli_path,
            "#!/bin/sh\nif [ \"$1\" = \"status\" ] && [ \"$2\" = \"--json\" ]; then\n  printf '%s' '{\"BackendState\":\"Running\",\"TailscaleIPs\":[\"100.64.9.9\"]}'\nelse\n  exit 1\nfi\n",
        )
        .unwrap();
        std::fs::set_permissions(&cli_path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let probe = SystemTailscaleProbe::with_paths_for_test(
            sock_path.to_string_lossy(),
            cli_path.to_string_lossy(),
        );
        let status = probe.fetch_status().await.unwrap();
        server.await.unwrap();

        assert_eq!(status.summary.backend_state, "Running");
        assert_eq!(status.summary.tailscale_ips, vec!["100.64.9.9"]);
    }
}
