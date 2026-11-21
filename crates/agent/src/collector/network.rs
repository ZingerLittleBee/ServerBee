use sysinfo::Networks;

/// Returns `true` for virtual Tailscale TUN interfaces (e.g. `tailscale`,
/// `tailscale0`, `Tailscale`), whose byte counters mirror encrypted WireGuard
/// packets already counted on the underlying physical NIC.
pub fn is_tailscale_interface(name: &str) -> bool {
    name.get(.."tailscale".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("tailscale"))
}

pub fn sum_network_bytes<'a, I>(interfaces: I, exclude_tailscale: bool) -> (u64, u64)
where
    I: IntoIterator<Item = (&'a str, u64, u64)>,
{
    let (mut total_in, mut total_out) = (0u64, 0u64);
    for (name, rx, tx) in interfaces {
        if exclude_tailscale && is_tailscale_interface(name) {
            continue;
        }
        total_in = total_in.saturating_add(rx);
        total_out = total_out.saturating_add(tx);
    }
    (total_in, total_out)
}

pub fn total_bytes(networks: &Networks, exclude_tailscale: bool) -> (u64, u64) {
    sum_network_bytes(
        networks.iter().map(|(name, data)| {
            (
                name.as_str(),
                data.total_received(),
                data.total_transmitted(),
            )
        }),
        exclude_tailscale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_tailscale_interface_matches_virtual_tun_names() {
        assert!(is_tailscale_interface("tailscale"));
        assert!(is_tailscale_interface("tailscale0"));
        assert!(is_tailscale_interface("tailscale1"));
        assert!(is_tailscale_interface("Tailscale"));
        assert!(is_tailscale_interface("TAILSCALE0"));
        assert!(!is_tailscale_interface("eth0"));
        assert!(!is_tailscale_interface("en0"));
        assert!(!is_tailscale_interface("utun3"));
        assert!(!is_tailscale_interface("wg0"));
        assert!(!is_tailscale_interface("lo"));
    }

    #[test]
    fn test_sum_network_bytes_excludes_tailscale_when_enabled() {
        let sample = [
            ("eth0", 1_000u64, 2_000u64),
            ("tailscale0", 500, 600),
            ("Tailscale", 100, 200),
            ("wlan0", 300, 400),
        ];

        assert_eq!(sum_network_bytes(sample, true), (1_300, 2_400));
        assert_eq!(sum_network_bytes(sample, false), (1_900, 3_200));
    }

    #[test]
    fn test_total_bytes_empty_networks_is_zero() {
        let networks = Networks::new();
        let (rx, tx) = total_bytes(&networks, true);
        assert_eq!(rx, 0);
        assert_eq!(tx, 0);
    }

    #[test]
    fn test_total_bytes_refreshed_list_is_consistent() {
        let networks = Networks::new_with_refreshed_list();
        let (rx_all, tx_all) = total_bytes(&networks, false);
        let (rx_filtered, tx_filtered) = total_bytes(&networks, true);

        let mut sum_in = 0u64;
        let mut sum_out = 0u64;
        for data in networks.values() {
            sum_in += data.total_received();
            sum_out += data.total_transmitted();
        }
        assert_eq!(rx_all, sum_in);
        assert_eq!(tx_all, sum_out);
        assert!(rx_filtered <= rx_all);
        assert!(tx_filtered <= tx_all);
    }
}
