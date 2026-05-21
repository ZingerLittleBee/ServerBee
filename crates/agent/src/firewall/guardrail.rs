//! Agent-side guardrail (tier 3, § 4.3). Subset of server-side:
//! hard-coded protected CIDRs + the agent's own external IP.

use std::net::IpAddr;

use ipnet::IpNet;

const PROTECTED: &[&str] = &[
    "127.0.0.0/8",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
    "0.0.0.0/8",
    "224.0.0.0/4",
    "::1/128",
    "fc00::/7",
    "fe80::/10",
    "ff00::/8",
    "::/128",
];

pub fn check(target_cidr: &str, own_external_ip: Option<IpAddr>) -> Result<(), String> {
    let net: IpNet = target_cidr
        .parse()
        .map_err(|_| format!("invalid CIDR: {target_cidr}"))?;
    for p in PROTECTED {
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
}
