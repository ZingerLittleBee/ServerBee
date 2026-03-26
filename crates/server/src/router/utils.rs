use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::HeaderMap;
use ipnet::IpNet;

/// Extract the real client IP address from a request.
///
/// Security model:
/// - If `trusted_proxies` is empty, return the TCP source IP directly (XFF ignored).
/// - If the TCP source IP is in `trusted_proxies`, walk `X-Forwarded-For` right-to-left
///   to find the first IP that is NOT in the trusted set.
/// - If no XFF header, or all IPs in the chain are trusted, return the TCP source IP.
///
/// This prevents spoofing: an untrusted client cannot forge XFF to bypass rate limiting.
pub fn extract_client_ip(
    connect_info: &ConnectInfo<SocketAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> IpAddr {
    let tcp_ip = connect_info.0.ip();

    // If no trusted proxies configured, never trust XFF — return TCP source directly.
    if trusted_proxies.is_empty() {
        return tcp_ip;
    }

    // If the TCP source is not a trusted proxy, XFF could be spoofed — return TCP source.
    if !is_trusted(tcp_ip, trusted_proxies) {
        return tcp_ip;
    }

    // TCP source is a trusted proxy — parse XFF right-to-left for the first untrusted IP.
    if let Some(xff) = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        // Walk the comma-separated list from right to left.
        for part in xff.split(',').rev() {
            let trimmed = part.trim();
            if let Ok(ip) = trimmed.parse::<IpAddr>() && !is_trusted(ip, trusted_proxies) {
                return ip;
            }
        }
    }

    // XFF didn't yield a non-trusted IP — return TCP source.
    // We intentionally do NOT fall back to X-Real-IP here: if the reverse proxy
    // merely forwards (rather than overwrites) client-supplied X-Real-IP, an
    // attacker behind a trusted proxy could still spoof it.
    tcp_ip
}

fn is_trusted(ip: IpAddr, proxies: &[IpNet]) -> bool {
    proxies.iter().any(|net| net.contains(&ip))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use axum::extract::ConnectInfo;
    use axum::http::HeaderMap;
    use ipnet::IpNet;

    use super::extract_client_ip;

    fn make_connect_info(ip: &str) -> ConnectInfo<SocketAddr> {
        let addr: SocketAddr = format!("{ip}:12345").parse().unwrap();
        ConnectInfo(addr)
    }

    fn make_cidr(cidr: &str) -> IpNet {
        cidr.parse().unwrap()
    }

    fn headers_with_xff(xff: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", xff.parse().unwrap());
        h
    }

    #[test]
    fn no_trusted_proxies_returns_tcp_ip() {
        // XFF is present but should be ignored when no proxies configured.
        let ci = make_connect_info("1.2.3.4");
        let headers = headers_with_xff("9.9.9.9");
        let ip = extract_client_ip(&ci, &headers, &[]);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn trusted_proxy_reads_xff_rightmost_untrusted() {
        // TCP source is the proxy (10.0.0.1), real client is 5.6.7.8.
        let ci = make_connect_info("10.0.0.1");
        let proxies = vec![make_cidr("10.0.0.0/8")];
        // XFF chain: client → proxy1 (both trusted proxies added their own)
        // The rightmost untrusted entry is the real client.
        let headers = headers_with_xff("5.6.7.8, 10.0.0.1");
        let ip = extract_client_ip(&ci, &headers, &proxies);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8)));
    }

    #[test]
    fn trusted_proxy_skips_all_trusted_in_chain() {
        // All entries in XFF are trusted — should fall back to TCP source IP.
        let ci = make_connect_info("10.0.0.1");
        let proxies = vec![make_cidr("10.0.0.0/8")];
        // Both IPs are in the 10.x.x.x trusted range.
        let headers = headers_with_xff("10.0.0.5, 10.0.0.1");
        // All XFF entries trusted — returns TCP source.
        let ip = extract_client_ip(&ci, &headers, &proxies);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    }

    #[test]
    fn untrusted_source_ignores_xff() {
        // TCP source is NOT a trusted proxy — XFF is ignored.
        let ci = make_connect_info("203.0.113.5");
        let proxies = vec![make_cidr("10.0.0.0/8")];
        let headers = headers_with_xff("1.1.1.1");
        let ip = extract_client_ip(&ci, &headers, &proxies);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)));
    }

    #[test]
    fn no_xff_header_returns_tcp_ip() {
        // Trusted proxy but no XFF header — returns TCP source.
        let ci = make_connect_info("10.0.0.1");
        let proxies = vec![make_cidr("10.0.0.0/8")];
        let headers = HeaderMap::new();
        let ip = extract_client_ip(&ci, &headers, &proxies);
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)));
    }

    #[test]
    fn spoofed_xff_from_untrusted_client_ignored() {
        // Attacker sends XFF trying to pretend they're 127.0.0.1, but they connect from
        // an untrusted IP — the forged header must be ignored.
        let ci = make_connect_info("198.51.100.9");
        let proxies = vec![make_cidr("10.0.0.0/8")];
        let headers = headers_with_xff("127.0.0.1");
        let ip = extract_client_ip(&ci, &headers, &proxies);
        // Must return the actual TCP source, not the spoofed 127.0.0.1.
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(198, 51, 100, 9)));
    }
}
