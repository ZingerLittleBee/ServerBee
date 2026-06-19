use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use anyhow::{Result, bail};
use url::Url;

/// Allowed URL schemes for outbound check requests.
const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// Allowed explicit ports (absent means scheme default — 80/443).
const ALLOWED_PORTS: &[u16] = &[80, 443];

/// Shared URL validation: scheme must be `http`/`https`, no embedded
/// credentials, and—when `restrict_ports` is set—the port must be 80/443 or the
/// scheme default. Returns the parsed `Url` on success.
fn validate_url_inner(raw: &str, restrict_ports: bool) -> Result<Url> {
    let url = Url::parse(raw)?;

    if !ALLOWED_SCHEMES.contains(&url.scheme()) {
        bail!(
            "SSRF guard: scheme '{}' is not allowed (only http/https)",
            url.scheme()
        );
    }

    if restrict_ports && url.port().is_some_and(|port| !ALLOWED_PORTS.contains(&port)) {
        bail!(
            "SSRF guard: port {} is not allowed (only 80/443 or scheme default)",
            url.port().unwrap()
        );
    }

    // Reject embedded credentials: they are not a guard bypass (the host is
    // still resolved and checked) but they would leak into the request and
    // logs.
    if !url.username().is_empty() || url.password().is_some() {
        bail!("SSRF guard: URL must not contain embedded credentials");
    }

    Ok(url)
}

/// Validate that a URL is safe to fetch on the strict (global-only) path:
///   - scheme must be `http` or `https`
///   - port must be 80, 443, or absent (scheme default)
///   - no embedded credentials (`user:pass@host`)
///
/// Returns the parsed `Url` on success.
pub fn validate_url(raw: &str) -> Result<Url> {
    validate_url_inner(raw, true)
}

/// Like [`validate_url`] but allows any port, for the service-monitor checkers
/// where operators legitimately monitor HTTP services on non-standard ports
/// (e.g. `:8080`, `:3000`). The scheme and embedded-credentials checks still
/// apply, and the address-level guard ([`is_monitor_safe_addr`]) still blocks
/// loopback/link-local/metadata regardless of port.
pub fn validate_monitor_url(raw: &str) -> Result<Url> {
    validate_url_inner(raw, false)
}

/// Returns `true` if `addr` is globally routable (safe to connect to).
///
/// Rejects:
///   IPv4: this-network (0.0.0.0/8), loopback (127.0.0.0/8),
///         private (10/8, 172.16/12, 192.168/16),
///         link-local (169.254.0.0/16), broadcast (255.255.255.255),
///         documentation (192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24),
///         shared address space (100.64.0.0/10)
///   IPv6: IPv4-mapped/-compatible (`::ffff:a.b.c.d` / `::a.b.c.d` — unwrapped
///         and re-checked through the IPv4 rules), loopback (::1),
///         unspecified (::), link-local (fe80::/10), ULA (fc00::/7),
///         documentation (2001:db8::/32), NAT64 well-known prefix
///         (64:ff9b::/96, RFC 6052)
pub fn is_global_addr(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return false;
            }
            if v4.is_private() {
                return false;
            }
            if v4.is_link_local() {
                return false;
            }
            if v4.is_broadcast() {
                return false;
            }
            let octets = v4.octets();
            // "This network" (RFC 791): 0.0.0.0/8 — covers 0.0.0.0 as well.
            if octets[0] == 0 {
                return false;
            }
            // Documentation ranges: 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
            if octets[0] == 192 && octets[1] == 0 && octets[2] == 2 {
                return false;
            }
            if octets[0] == 198 && octets[1] == 51 && octets[2] == 100 {
                return false;
            }
            if octets[0] == 203 && octets[1] == 0 && octets[2] == 113 {
                return false;
            }
            // Shared address space (RFC 6598): 100.64.0.0/10
            if octets[0] == 100 && (octets[1] & 0b1100_0000) == 0b0100_0000 {
                return false;
            }
            true
        }
        IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped (`::ffff:a.b.c.d`) and IPv4-compatible
            // (`::a.b.c.d`) addresses and re-check through the IPv4 rules.
            // Without this, `[::ffff:127.0.0.1]` would slip past the v6
            // checks (its `.is_loopback()` is false) and defeat the guard.
            if let Some(v4) = v6.to_ipv4() {
                return is_global_addr(IpAddr::V4(v4));
            }
            if v6.is_loopback() {
                return false;
            }
            if v6.is_unspecified() {
                return false;
            }
            let segs = v6.segments();
            // Link-local: fe80::/10 — first 10 bits are 1111111010
            if (segs[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            // ULA: fc00::/7 — first 7 bits are 1111110
            if (segs[0] & 0xfe00) == 0xfc00 {
                return false;
            }
            // Documentation: 2001:db8::/32 — first two segments are 2001:0db8
            if segs[0] == 0x2001 && segs[1] == 0x0db8 {
                return false;
            }
            // NAT64 well-known prefix: 64:ff9b::/96 (RFC 6052). The low 32
            // bits embed an IPv4 address (e.g. 64:ff9b::7f00:1 = 127.0.0.1),
            // so the whole /96 is rejected.
            if segs[0] == 0x0064
                && segs[1] == 0xff9b
                && segs[2] == 0
                && segs[3] == 0
                && segs[4] == 0
                && segs[5] == 0
            {
                return false;
            }
            true
        }
    }
}

/// Resolve `host` to its socket addresses (on `port`) and reject if **any**
/// resolved address is non-global. This is the DNS-rebinding defense: if any
/// address is private the host is unsafe.
///
/// Returns the list of resolved `SocketAddr` on success. Callers should connect
/// to the returned addresses directly (rather than re-resolving the host) to
/// keep the validated result and the connected address identical.
pub fn resolve_and_check(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = (host, port).to_socket_addrs()?.collect();

    if addrs.is_empty() {
        bail!("SSRF guard: could not resolve host '{}'", host);
    }

    for addr in &addrs {
        if !is_global_addr(addr.ip()) {
            bail!(
                "SSRF guard: host '{}' resolved to non-global address {} — request blocked",
                host,
                addr.ip()
            );
        }
    }

    Ok(addrs)
}

/// Returns `true` if `addr` is safe for the **service-monitor checkers** to
/// connect to.
///
/// Unlike [`is_global_addr`], this intentionally ALLOWS RFC1918 private ranges
/// (10/8, 172.16/12, 192.168/16) and IPv6 ULA (fc00::/7) so operators can
/// legitimately monitor internal/LAN hosts. It still blocks the addresses that
/// have no business being a monitoring target and are the real SSRF prizes:
/// loopback, link-local (incl. the cloud-metadata 169.254.169.254), unspecified,
/// broadcast, the NAT64 well-known prefix, and the IPv4-mapped/-compatible IPv6
/// forms of any of those.
pub fn is_monitor_safe_addr(addr: IpAddr) -> bool {
    match addr {
        IpAddr::V4(v4) => {
            if v4.is_loopback() {
                return false;
            }
            // 169.254.0.0/16 — link-local, incl. the cloud metadata endpoint.
            if v4.is_link_local() {
                return false;
            }
            if v4.is_broadcast() {
                return false;
            }
            // "This network" (RFC 791): 0.0.0.0/8.
            if v4.octets()[0] == 0 {
                return false;
            }
            // RFC1918 private ranges are intentionally ALLOWED for internal monitoring.
            true
        }
        IpAddr::V6(v6) => {
            // Unwrap IPv4-mapped/-compatible forms and re-check through the v4
            // rules so `[::ffff:169.254.169.254]` cannot slip past.
            if let Some(v4) = v6.to_ipv4() {
                return is_monitor_safe_addr(IpAddr::V4(v4));
            }
            if v6.is_loopback() {
                return false;
            }
            if v6.is_unspecified() {
                return false;
            }
            let segs = v6.segments();
            // Link-local: fe80::/10.
            if (segs[0] & 0xffc0) == 0xfe80 {
                return false;
            }
            // NAT64 well-known prefix (64:ff9b::/96) embeds an IPv4 address and
            // can reach metadata/loopback over IPv6 — block it wholesale.
            if segs[0] == 0x0064
                && segs[1] == 0xff9b
                && segs[2] == 0
                && segs[3] == 0
                && segs[4] == 0
                && segs[5] == 0
            {
                return false;
            }
            // ULA (fc00::/7) is the IPv6 analogue of private space — ALLOWED.
            true
        }
    }
}

/// Like [`resolve_and_check`] but uses [`is_monitor_safe_addr`] (allows private
/// ranges; blocks loopback/link-local/metadata/NAT64). For the service-monitor
/// checkers, which legitimately need to reach internal hosts.
///
/// Returns the validated `SocketAddr`s; callers should connect to them directly
/// (without re-resolving the host) to close the DNS-rebinding window.
pub fn resolve_and_check_monitor(host: &str, port: u16) -> Result<Vec<SocketAddr>> {
    let addrs: Vec<SocketAddr> = (host, port).to_socket_addrs()?.collect();

    if addrs.is_empty() {
        bail!("SSRF guard: could not resolve host '{}'", host);
    }

    for addr in &addrs {
        if !is_monitor_safe_addr(addr.ip()) {
            bail!(
                "SSRF guard: host '{}' resolved to blocked address {} (loopback/link-local/metadata) — request blocked",
                host,
                addr.ip()
            );
        }
    }

    Ok(addrs)
}

/// Extract the host component from a probe/monitor target string. Handles
/// `http(s)://host[:port]/path` URLs, `[ipv6]` / `[ipv6]:port`, `host:port`,
/// bare IPv6 literals, and bare hosts. Returns the target unchanged if no host
/// can be isolated.
fn extract_target_host(target: &str) -> String {
    if target.contains("://")
        && let Ok(url) = Url::parse(target)
        && let Some(h) = url.host_str()
    {
        // url returns bracketed IPv6 (`[::1]`); strip brackets so it parses as Ip.
        return h.trim_start_matches('[').trim_end_matches(']').to_string();
    }
    if let Some(rest) = target.strip_prefix('[') {
        if let Some((h, _)) = rest.split_once("]:") {
            return h.to_string();
        }
        if let Some(h) = rest.strip_suffix(']') {
            return h.to_string();
        }
    }
    if let Some((h, port)) = target.rsplit_once(':')
        && !h.contains(':')
        && !port.is_empty()
        && port.chars().all(|c| c.is_ascii_digit())
    {
        return h.to_string();
    }
    target.to_string()
}

/// Server-side, DNS-free guard for probe/monitor targets. Rejects targets whose
/// host is a **literal** non-monitor-safe IP (loopback / link-local incl. cloud
/// metadata / NAT64 / broadcast / this-network). Domain names are accepted here
/// (they are validated at probe time on the agent, which resolves them) and
/// RFC1918 private ranges are accepted so internal monitoring keeps working.
/// This is defense-in-depth + immediate operator feedback, not the primary
/// guard (which lives agent-side in `probe_utils`).
pub fn reject_literal_unsafe_target(target: &str) -> Result<()> {
    let host = extract_target_host(target);
    if let Ok(ip) = host.parse::<IpAddr>()
        && !is_monitor_safe_addr(ip)
    {
        bail!(
            "SSRF guard: target '{}' points at a blocked address ({}) — loopback/link-local/metadata targets are not allowed",
            target,
            ip
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_url ──────────────────────────────────────────────────────────

    #[test]
    fn validate_url_accepts_http_default_port() {
        assert!(validate_url("http://example.com/check").is_ok());
    }

    #[test]
    fn validate_url_accepts_https_default_port() {
        assert!(validate_url("https://example.com/check").is_ok());
    }

    #[test]
    fn validate_url_accepts_explicit_port_80() {
        assert!(validate_url("http://example.com:80/check").is_ok());
    }

    #[test]
    fn validate_url_accepts_explicit_port_443() {
        assert!(validate_url("https://example.com:443/check").is_ok());
    }

    #[test]
    fn validate_url_rejects_non_http_scheme_ftp() {
        let err = validate_url("ftp://example.com/file").unwrap_err();
        assert!(
            err.to_string().contains("scheme"),
            "expected scheme error, got: {err}"
        );
    }

    #[test]
    fn validate_url_rejects_non_http_scheme_file() {
        assert!(validate_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn validate_url_rejects_non_http_scheme_gopher() {
        assert!(validate_url("gopher://example.com/").is_err());
    }

    #[test]
    fn validate_url_rejects_port_8080() {
        let err = validate_url("http://example.com:8080/").unwrap_err();
        assert!(
            err.to_string().contains("port"),
            "expected port error, got: {err}"
        );
    }

    #[test]
    fn validate_url_rejects_port_3000() {
        assert!(validate_url("http://example.com:3000/").is_err());
    }

    #[test]
    fn validate_url_rejects_embedded_username() {
        let err = validate_url("http://user@example.com/").unwrap_err();
        assert!(
            err.to_string().contains("credentials"),
            "expected credentials error, got: {err}"
        );
    }

    #[test]
    fn validate_url_rejects_embedded_user_and_password() {
        let err = validate_url("http://user:pass@example.com/").unwrap_err();
        assert!(
            err.to_string().contains("credentials"),
            "expected credentials error, got: {err}"
        );
    }

    // ── validate_monitor_url (any port; scheme/credentials still enforced) ────

    #[test]
    fn validate_monitor_url_allows_custom_port() {
        // The whole point of the relaxed validator: non-standard ports work.
        assert!(validate_monitor_url("http://example.com:8080/health").is_ok());
        assert!(validate_monitor_url("https://example.com:3000/").is_ok());
        assert!(validate_monitor_url("http://example.com:9000/").is_ok());
    }

    #[test]
    fn validate_monitor_url_still_allows_standard_ports() {
        assert!(validate_monitor_url("http://example.com/").is_ok());
        assert!(validate_monitor_url("https://example.com:443/").is_ok());
    }

    #[test]
    fn validate_monitor_url_still_rejects_non_http_scheme() {
        assert!(validate_monitor_url("file:///etc/passwd").is_err());
        assert!(validate_monitor_url("gopher://example.com:8080/").is_err());
    }

    #[test]
    fn validate_monitor_url_still_rejects_embedded_credentials() {
        let err = validate_monitor_url("http://user:pass@example.com:8080/").unwrap_err();
        assert!(
            err.to_string().contains("credentials"),
            "expected credentials error, got: {err}"
        );
    }

    // ── is_global_addr ────────────────────────────────────────────────────────

    #[test]
    fn is_global_addr_rejects_ipv4_loopback() {
        assert!(!is_global_addr("127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_private_10() {
        assert!(!is_global_addr("10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_private_192_168() {
        assert!(!is_global_addr("192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_link_local() {
        assert!(!is_global_addr("169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_this_network() {
        // 0.0.0.0/8 — the whole "this network" range, not just 0.0.0.0.
        assert!(!is_global_addr("0.0.0.0".parse().unwrap()));
        assert!(!is_global_addr("0.1.2.3".parse().unwrap()));
        assert!(!is_global_addr("0.255.255.255".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_accepts_ipv4_public() {
        assert!(is_global_addr("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv6_loopback() {
        assert!(!is_global_addr("::1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv6_ula() {
        assert!(!is_global_addr("fc00::1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv6_link_local() {
        assert!(!is_global_addr("fe80::1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_accepts_ipv6_public() {
        assert!(is_global_addr("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_mapped_loopback() {
        // ::ffff:127.0.0.1 — IPv4-mapped, must be unwrapped and rejected.
        assert!(!is_global_addr("::ffff:127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_mapped_private() {
        assert!(!is_global_addr("::ffff:10.0.0.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_mapped_metadata_ip() {
        // ::ffff:169.254.169.254 — cloud metadata via IPv4-mapped form.
        assert!(!is_global_addr("::ffff:169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv4_compatible_loopback() {
        // ::127.0.0.1 — IPv4-compatible form, must also be unwrapped.
        assert!(!is_global_addr("::127.0.0.1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv6_documentation() {
        // 2001:db8::/32 — IPv6 documentation range.
        assert!(!is_global_addr("2001:db8::1".parse().unwrap()));
    }

    #[test]
    fn is_global_addr_rejects_ipv6_nat64_well_known_prefix() {
        // 64:ff9b::/96 embeds an IPv4 address; 64:ff9b::7f00:1 = 127.0.0.1.
        assert!(!is_global_addr("64:ff9b::7f00:1".parse().unwrap()));
    }

    // ── resolve_and_check ─────────────────────────────────────────────────────

    #[test]
    fn resolve_and_check_rejects_localhost() {
        let err = resolve_and_check("localhost", 80).unwrap_err();
        assert!(
            err.to_string().contains("SSRF guard"),
            "expected SSRF guard error, got: {err}"
        );
    }

    #[test]
    fn resolve_and_check_rejects_ipv4_loopback_literal() {
        let err = resolve_and_check("127.0.0.1", 80).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn resolve_and_check_rejects_ipv4_private() {
        let err = resolve_and_check("10.0.0.1", 80).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn resolve_and_check_rejects_link_local_metadata_ip() {
        let err = resolve_and_check("169.254.169.254", 80).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn resolve_and_check_rejects_ipv6_loopback() {
        let err = resolve_and_check("::1", 80).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn resolve_and_check_rejects_ipv6_ula() {
        let err = resolve_and_check("fc00::1", 80).unwrap_err();
        assert!(err.to_string().contains("SSRF guard"), "got: {err}");
    }

    #[test]
    fn resolve_and_check_accepts_public_ipv4() {
        let result = resolve_and_check("8.8.8.8", 80);
        assert!(result.is_ok(), "expected success, got: {:?}", result.err());
    }

    // ── is_monitor_safe_addr (private allowed, metadata/loopback blocked) ──────

    #[test]
    fn monitor_safe_blocks_loopback() {
        assert!(!is_monitor_safe_addr("127.0.0.1".parse().unwrap()));
        assert!(!is_monitor_safe_addr("::1".parse().unwrap()));
    }

    #[test]
    fn monitor_safe_blocks_cloud_metadata() {
        assert!(!is_monitor_safe_addr("169.254.169.254".parse().unwrap()));
        assert!(!is_monitor_safe_addr("::ffff:169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn monitor_safe_blocks_ipv6_link_local_and_nat64() {
        assert!(!is_monitor_safe_addr("fe80::1".parse().unwrap()));
        // 64:ff9b::a9fe:a9fe = NAT64-wrapped 169.254.169.254.
        assert!(!is_monitor_safe_addr("64:ff9b::a9fe:a9fe".parse().unwrap()));
    }

    #[test]
    fn monitor_safe_allows_rfc1918_private() {
        // Internal/LAN monitoring is a legitimate use case — these are allowed.
        assert!(is_monitor_safe_addr("10.0.0.5".parse().unwrap()));
        assert!(is_monitor_safe_addr("172.16.0.1".parse().unwrap()));
        assert!(is_monitor_safe_addr("192.168.1.10".parse().unwrap()));
        assert!(is_monitor_safe_addr("fc00::1".parse().unwrap()));
    }

    #[test]
    fn monitor_safe_allows_public() {
        assert!(is_monitor_safe_addr("8.8.8.8".parse().unwrap()));
    }

    #[test]
    fn resolve_and_check_monitor_blocks_metadata_allows_private() {
        assert!(resolve_and_check_monitor("169.254.169.254", 80).is_err());
        assert!(resolve_and_check_monitor("127.0.0.1", 80).is_err());
        assert!(resolve_and_check_monitor("10.0.0.5", 80).is_ok());
    }

    // ── reject_literal_unsafe_target (server-side, DNS-free) ──────────────────

    #[test]
    fn reject_literal_unsafe_target_blocks_loopback() {
        assert!(reject_literal_unsafe_target("127.0.0.1").is_err());
        assert!(reject_literal_unsafe_target("127.0.0.1:8080").is_err());
        assert!(reject_literal_unsafe_target("::1").is_err());
    }

    #[test]
    fn reject_literal_unsafe_target_blocks_cloud_metadata() {
        assert!(reject_literal_unsafe_target("169.254.169.254").is_err());
        assert!(reject_literal_unsafe_target("169.254.169.254:80").is_err());
        assert!(reject_literal_unsafe_target("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(reject_literal_unsafe_target("http://[::1]/").is_err());
    }

    #[test]
    fn reject_literal_unsafe_target_allows_public_and_domains() {
        // Public literals and domain names (resolved/validated agent-side) pass.
        assert!(reject_literal_unsafe_target("8.8.8.8").is_ok());
        assert!(reject_literal_unsafe_target("8.8.8.8:53").is_ok());
        assert!(reject_literal_unsafe_target("example.com").is_ok());
        assert!(reject_literal_unsafe_target("example.com:8080").is_ok());
        assert!(reject_literal_unsafe_target("https://example.com:8443/health").is_ok());
    }

    #[test]
    fn reject_literal_unsafe_target_allows_rfc1918_for_internal_monitoring() {
        assert!(reject_literal_unsafe_target("10.0.0.1").is_ok());
        assert!(reject_literal_unsafe_target("192.168.1.1:80").is_ok());
        assert!(reject_literal_unsafe_target("172.16.5.5").is_ok());
    }
}
