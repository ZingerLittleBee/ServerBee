//! Agent-side guardrail (tier 3, § 4.3). Subset of server-side:
//! hard-coded protected CIDRs + the agent's own external IP.

use std::net::IpAddr;

use ipnet::IpNet;
use serverbee_common::firewall::PROTECTED_CIDRS;

pub fn check(target_cidr: &str, own_external_ip: Option<IpAddr>) -> Result<(), String> {
    let net: IpNet = target_cidr
        .parse()
        .map_err(|_| format!("invalid CIDR: {target_cidr}"))?;
    for p in PROTECTED_CIDRS {
        let prot: IpNet = p.parse().expect("hard-coded valid");
        if prot.contains(&net.network()) || net.contains(&prot.network()) {
            return Err(format!("guardrail: {p}"));
        }
    }
    if let Some(ip) = own_external_ip {
        let own_net = IpNet::new(ip, if ip.is_ipv4() { 32 } else { 128 }).expect("valid prefix");
        if own_net.network() == net.network() && own_net.prefix_len() == net.prefix_len() {
            return Err(format!("guardrail: agent's own external IP {ip}"));
        }
        if net.contains(&ip) {
            return Err(format!("guardrail: range contains own external IP {ip}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_loopback() {
        assert!(check("127.0.0.1/32", None).is_err());
    }

    #[test]
    fn rejects_own_ip() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(check("203.0.113.7/32", Some(ip)).is_err());
    }

    #[test]
    fn rejects_range_containing_own_ip() {
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(check("203.0.113.0/24", Some(ip)).is_err());
    }

    #[test]
    fn accepts_external_unrelated() {
        assert!(check("198.51.100.5/32", None).is_ok());
    }

    #[test]
    fn rejects_invalid_cidr() {
        let err = check("not-a-cidr", None).unwrap_err();
        assert!(err.contains("invalid CIDR"));
    }

    #[test]
    fn rejects_protected_when_target_contains_protected_range() {
        // Target 10.0.0.0/8 exactly equals a protected CIDR; covers the
        // `prot.contains(net.network())` direction of the guardrail check.
        let err = check("10.0.0.0/8", None).unwrap_err();
        assert!(err.starts_with("guardrail:"));
    }

    #[test]
    fn rejects_protected_when_target_is_superset_of_protected_range() {
        // A /4 that contains the 224.0.0.0/4 multicast block exercises the
        // `net.contains(prot.network())` direction.
        let err = check("224.0.0.0/3", None).unwrap_err();
        assert!(err.starts_with("guardrail:"));
    }

    #[test]
    fn rejects_own_ipv6() {
        let ip: IpAddr = "2001:db8::1".parse().unwrap();
        let err = check("2001:db8::1/128", Some(ip)).unwrap_err();
        assert!(err.contains("own external IP"));
    }

    #[test]
    fn rejects_ipv6_range_containing_own_ip() {
        let ip: IpAddr = "2001:db8::5".parse().unwrap();
        let err = check("2001:db8::/64", Some(ip)).unwrap_err();
        assert!(err.contains("range contains own external IP"));
    }

    #[test]
    fn accepts_external_with_unrelated_own_ip() {
        // Own IP provided but neither equal to nor contained by the target;
        // covers the success fall-through with `own_external_ip = Some(_)`.
        let ip: IpAddr = "203.0.113.7".parse().unwrap();
        assert!(check("198.51.100.5/32", Some(ip)).is_ok());
    }
}
