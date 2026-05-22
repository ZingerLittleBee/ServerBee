// These items are used by http.rs and by later units (detectors, scheduler).
// The dead_code lint fires now because ip_quality is not yet wired into
// main.rs's runtime path.
#![allow(dead_code)]

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};

use anyhow::{bail, Result};
use url::Url;

/// Allowed URL schemes for unlock check requests.
const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// Allowed explicit ports (absent means scheme default — 80/443).
const ALLOWED_PORTS: &[u16] = &[80, 443];

/// Validate that a URL is safe to fetch:
///   - scheme must be `http` or `https`
///   - port must be 80, 443, or absent (scheme default)
///   - no embedded credentials (`user:pass@host`)
///
/// Returns the parsed `Url` on success.
pub fn validate_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw)?;

    if !ALLOWED_SCHEMES.contains(&url.scheme()) {
        bail!("SSRF guard: scheme '{}' is not allowed (only http/https)", url.scheme());
    }

    if url.port().is_some_and(|port| !ALLOWED_PORTS.contains(&port)) {
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
/// Returns the list of resolved `SocketAddr` on success.
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
        assert!(err.to_string().contains("scheme"), "expected scheme error, got: {err}");
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
        assert!(err.to_string().contains("port"), "expected port error, got: {err}");
    }

    #[test]
    fn validate_url_rejects_port_3000() {
        assert!(validate_url("http://example.com:3000/").is_err());
    }

    #[test]
    fn validate_url_rejects_embedded_username() {
        let err = validate_url("http://user@example.com/").unwrap_err();
        assert!(err.to_string().contains("credentials"), "expected credentials error, got: {err}");
    }

    #[test]
    fn validate_url_rejects_embedded_user_and_password() {
        let err = validate_url("http://user:pass@example.com/").unwrap_err();
        assert!(err.to_string().contains("credentials"), "expected credentials error, got: {err}");
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
}
