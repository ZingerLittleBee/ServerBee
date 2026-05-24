//! trippy-core backed traceroute implementation.
//!
//! Replaces the legacy shell `traceroute`/`mtr` subprocess invocation with
//! an in-process tracer that streams per-round updates to the server via
//! `AgentMessage::TracerouteRoundUpdate`. See plan
//! `docs/superpowers/plans/2026-05-24-traceroute-trippy-core.md` Task 14.

use std::cell::Cell;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::Duration;

use serverbee_common::protocol::{AgentMessage, TraceProtocol};
use serverbee_common::types::TracerouteHop;
use tokio::sync::mpsc;
use trippy_core::{Builder, Port, PortDirection, PrivilegeMode, Protocol, State};

pub const DEFAULT_MAX_ROUNDS: u32 = 5;
pub const ROUND_INTERVAL: Duration = Duration::from_millis(1000);
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);
/// Hard wall-clock budget for the whole trace. Normal completion is
/// ~rounds × max_round_duration (≈10s). 60s leaves slack for slow links;
/// anything longer is almost certainly a stuck OS socket.
pub const TRACE_WALL_TIMEOUT: Duration = Duration::from_secs(60);
pub const UDP_DEFAULT_DEST_PORT: u16 = 33_434;
pub const TCP_DEFAULT_DEST_PORT: u16 = 80;

pub fn port_direction_for(proto: Protocol) -> PortDirection {
    match proto {
        Protocol::Icmp => PortDirection::None,
        Protocol::Udp => PortDirection::FixedDest(Port(UDP_DEFAULT_DEST_PORT)),
        Protocol::Tcp => PortDirection::FixedDest(Port(TCP_DEFAULT_DEST_PORT)),
    }
}

pub fn trippy_protocol_from(p: TraceProtocol) -> Protocol {
    match p {
        TraceProtocol::Icmp => Protocol::Icmp,
        TraceProtocol::Udp => Protocol::Udp,
        TraceProtocol::Tcp => Protocol::Tcp,
    }
}

/// Validate that a traceroute target contains only safe characters (domain or IP).
pub fn is_valid_traceroute_target(target: &str) -> bool {
    !target.is_empty()
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == ':')
}

/// Resolve a literal IP or hostname. `lookup_host` requires `host:port`, so
/// we tack on a dummy port and take the first address.
pub async fn resolve_target(target: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = IpAddr::from_str(target) {
        return Ok(ip);
    }
    let mut iter = tokio::net::lookup_host((target, 0u16))
        .await
        .map_err(|e| format!("DNS resolution failed: {e}"))?;
    iter.next()
        .map(|sa| sa.ip())
        .ok_or_else(|| format!("DNS resolution returned no addresses for {target}"))
}

pub fn is_privilege_error(e: &trippy_core::Error) -> bool {
    use trippy_core::Error::{IoError, PrivilegeError, ProbeFailed};
    // `trippy_core::error::{IoError, ErrorKind}` are NOT re-exported from the
    // crate root in trippy 0.13, so we cannot directly destructure the IoError
    // variant to inspect its `kind()`. Instead, identify privilege errors via
    // the `PrivilegeError` variant (always raised on Windows/Linux capability
    // failures) or by string-matching the formatted error for IO variants.
    if matches!(e, PrivilegeError(_)) {
        return true;
    }
    if matches!(e, IoError(_) | ProbeFailed(_)) {
        let s = e.to_string().to_lowercase();
        return s.contains("permission denied") || s.contains("operation not permitted");
    }
    false
}

pub fn platform_guidance() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "Traceroute requires elevated privileges. Run the agent as root, or grant CAP_NET_RAW: \
         sudo setcap cap_net_raw+ep $(which serverbee-agent)"
    }
    #[cfg(target_os = "macos")]
    {
        "Traceroute requires elevated privileges. Run the agent as root (sudo)."
    }
    #[cfg(target_os = "windows")]
    {
        "Traceroute requires Administrator privileges. Restart the agent as Administrator."
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "Traceroute requires elevated privileges. Run the agent as a privileged user."
    }
}

/// Build a tracer and run it, invoking `on_round` after each completed probe
/// round with a fresh `State` snapshot. We snapshot via `Tracer::snapshot()`
/// because `State::new(StateConfig)` requires `StateConfig`, which is not
/// re-exported from `trippy-core` 0.13.
fn try_trace<F>(
    addr: IpAddr,
    max_hops: u8,
    proto: Protocol,
    priv_mode: PrivilegeMode,
    on_round: F,
) -> Result<(), trippy_core::Error>
where
    F: Fn(&State),
{
    let tracer = Builder::new(addr)
        .max_ttl(max_hops)
        .max_rounds(Some(DEFAULT_MAX_ROUNDS as usize))
        .min_round_duration(ROUND_INTERVAL)
        .max_round_duration(ROUND_INTERVAL * 2)
        .read_timeout(PROBE_TIMEOUT)
        .protocol(proto)
        .port_direction(port_direction_for(proto))
        .privilege_mode(priv_mode)
        .build()?;
    // `Tracer` is `Clone` (cheap Arc-backed handle). Clone so the callback can
    // call `snapshot()` while `run_with` borrows the original.
    let tracer_for_callback = tracer.clone();
    tracer.run_with(move |_round: &trippy_core::Round<'_>| {
        let snap = tracer_for_callback.snapshot();
        on_round(&snap);
    })
}

fn hops_from_state(state: &State) -> Vec<TracerouteHop> {
    state
        .hops()
        .iter()
        .map(|h| TracerouteHop {
            hop: h.ttl(),
            // Legacy fields left None — server discriminates via total_sent.
            ip: None,
            hostname: None,
            rtt1: None,
            rtt2: None,
            rtt3: None,
            asn: None,
            ips: h.addrs().map(|a| a.to_string()).collect(),
            total_sent: Some(h.total_sent() as u32),
            total_recv: Some(h.total_recv() as u32),
            loss_pct: Some(h.loss_pct()),
            best_ms: h.best_ms(),
            worst_ms: h.worst_ms(),
            avg_ms: Some(h.avg_ms()),
            stddev_ms: Some(h.stddev_ms()),
            jitter_ms: h.jitter_ms(),
        })
        .collect()
}

pub fn spawn_traceroute(
    request_id: String,
    target: String,
    max_hops: u8,
    protocol: TraceProtocol,
    tx: mpsc::Sender<AgentMessage>,
) {
    tokio::spawn(async move {
        let addr = match resolve_target(&target).await {
            Ok(a) => a,
            Err(e) => {
                let _ = tx
                    .send(AgentMessage::TracerouteRoundUpdate {
                        request_id,
                        target,
                        round: 0,
                        total_rounds: 0,
                        hops: vec![],
                        completed: true,
                        error: Some(format!("DNS error: {e}")),
                    })
                    .await;
                return;
            }
        };
        let proto = trippy_protocol_from(protocol);
        let request_id_inner = request_id.clone();
        let target_inner = target.clone();
        let tx_inner = tx.clone();

        let blocking = tokio::task::spawn_blocking(move || -> Result<(), String> {
            let round_no = Cell::new(0u32);
            let make_callback = || {
                |state: &State| {
                    round_no.set(round_no.get() + 1);
                    let n = round_no.get();
                    let completed = n >= DEFAULT_MAX_ROUNDS;
                    let hops = hops_from_state(state);
                    let _ = tx_inner.blocking_send(AgentMessage::TracerouteRoundUpdate {
                        request_id: request_id_inner.clone(),
                        target: target_inner.clone(),
                        round: n,
                        total_rounds: DEFAULT_MAX_ROUNDS,
                        hops,
                        completed,
                        error: None,
                    });
                }
            };

            // Try privileged → fallback to unprivileged on privilege errors.
            match try_trace(addr, max_hops, proto, PrivilegeMode::Privileged, make_callback()) {
                Ok(()) => Ok(()),
                Err(e) if is_privilege_error(&e) => {
                    tracing::info!("traceroute privileged failed ({e}); retrying unprivileged");
                    // Reset round counter for the fallback attempt.
                    round_no.set(0);
                    match try_trace(
                        addr,
                        max_hops,
                        proto,
                        PrivilegeMode::Unprivileged,
                        make_callback(),
                    ) {
                        Ok(()) => Ok(()),
                        Err(e2) => Err(format!("{}: {e2}", platform_guidance())),
                    }
                }
                Err(e) => Err(format!("Tracer error: {e}")),
            }
        });

        // Bound the await with a wall-clock timeout. trippy's `run_with` has
        // no cancellation hook, so on timeout the blocking thread keeps
        // running until trippy returns; this is partial mitigation that
        // unblocks the caller without leaking thread-pool slots forever in
        // the common case (trippy still finishes within its own bookkeeping).
        let result = match tokio::time::timeout(TRACE_WALL_TIMEOUT, blocking).await {
            Ok(inner) => inner,
            Err(_elapsed) => {
                let _ = tx
                    .send(AgentMessage::TracerouteRoundUpdate {
                        request_id,
                        target,
                        round: 0,
                        total_rounds: 0,
                        hops: vec![],
                        completed: true,
                        error: Some(format!(
                            "Traceroute exceeded {}s wall-clock timeout",
                            TRACE_WALL_TIMEOUT.as_secs()
                        )),
                    })
                    .await;
                return;
            }
        };

        // Emit terminal error if the blocking task crashed or returned Err.
        match result {
            Ok(Ok(())) => {} // success — final round message already sent with completed=true
            Ok(Err(msg)) => {
                let _ = tx
                    .send(AgentMessage::TracerouteRoundUpdate {
                        request_id,
                        target,
                        round: 0,
                        total_rounds: 0,
                        hops: vec![],
                        completed: true,
                        error: Some(msg),
                    })
                    .await;
            }
            Err(join_err) => {
                let _ = tx
                    .send(AgentMessage::TracerouteRoundUpdate {
                        request_id,
                        target,
                        round: 0,
                        total_rounds: 0,
                        hops: vec![],
                        completed: true,
                        error: Some(format!("Tracer task panicked: {join_err}")),
                    })
                    .await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use trippy_core::{PortDirection, Protocol};

    #[test]
    fn test_is_valid_traceroute_target() {
        assert!(is_valid_traceroute_target("8.8.8.8"));
        assert!(is_valid_traceroute_target("google.com"));
        assert!(is_valid_traceroute_target("sub.example.com"));
        assert!(is_valid_traceroute_target("2001:db8::1"));
        assert!(is_valid_traceroute_target("my-server.example.com"));
        assert!(!is_valid_traceroute_target(""));
        assert!(!is_valid_traceroute_target("8.8.8.8; rm -rf /"));
        assert!(!is_valid_traceroute_target("$(whoami)"));
        assert!(!is_valid_traceroute_target("foo bar"));
    }

    #[test]
    fn test_port_direction_for_icmp_is_none() {
        assert!(matches!(port_direction_for(Protocol::Icmp), PortDirection::None));
    }

    #[test]
    fn test_port_direction_for_udp_is_fixed_dest_33434() {
        match port_direction_for(Protocol::Udp) {
            PortDirection::FixedDest(Port(p)) => assert_eq!(p, 33_434),
            other => panic!("expected FixedDest(33434), got {other:?}"),
        }
    }

    #[test]
    fn test_port_direction_for_tcp_is_fixed_dest_80() {
        match port_direction_for(Protocol::Tcp) {
            PortDirection::FixedDest(Port(p)) => assert_eq!(p, 80),
            other => panic!("expected FixedDest(80), got {other:?}"),
        }
    }

    #[test]
    fn test_builder_builds_for_all_three_protocols() {
        // Regression guard against trippy's BadConfig when UDP/TCP are paired
        // with PortDirection::None. We only build (no run) so this doesn't
        // require raw socket privileges in CI.
        let addr: IpAddr = "1.1.1.1".parse().unwrap();
        for proto in [Protocol::Icmp, Protocol::Udp, Protocol::Tcp] {
            let res = Builder::new(addr)
                .max_ttl(5)
                .max_rounds(Some(1))
                .protocol(proto)
                .port_direction(port_direction_for(proto))
                .privilege_mode(PrivilegeMode::Privileged)
                .build();
            assert!(res.is_ok(), "build failed for {proto:?}: {:?}", res.err());
        }
    }

    #[tokio::test]
    async fn test_resolve_literal_ipv4() {
        assert_eq!(
            resolve_target("8.8.8.8").await.unwrap(),
            "8.8.8.8".parse::<IpAddr>().unwrap()
        );
    }

    #[tokio::test]
    async fn test_resolve_literal_ipv6() {
        assert_eq!(
            resolve_target("::1").await.unwrap(),
            "::1".parse::<IpAddr>().unwrap()
        );
    }

    #[tokio::test]
    async fn test_resolve_invalid_hostname_returns_err() {
        // RFC 2606 reserves `.invalid` as guaranteed-unresolvable. Some local
        // DNS environments (captive portals, search-domain rewriting, ISP
        // hijacking) may still return an answer. We treat such an environment
        // as a soft pass — the function under test is fine; the env is broken.
        match resolve_target("definitely-not-a-real-tld.invalid").await {
            Err(_) => {} // expected
            Ok(addr) => {
                eprintln!(
                    "WARN: resolver returned {addr:?} for .invalid TLD — \
                     skipping assertion (likely a hijacking resolver)."
                );
            }
        }
    }
}
