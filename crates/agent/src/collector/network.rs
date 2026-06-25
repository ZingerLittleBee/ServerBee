use sysinfo::Networks;

pub fn total_bytes(networks: &Networks) -> (u64, u64) {
    let (mut total_in, mut total_out) = (0u64, 0u64);
    for (_name, data) in networks.iter() {
        total_in += data.total_received();
        total_out += data.total_transmitted();
    }
    (total_in, total_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_total_bytes_empty_networks_is_zero() {
        let networks = Networks::new();
        let (rx, tx) = total_bytes(&networks);
        assert_eq!(rx, 0);
        assert_eq!(tx, 0);
    }

    #[test]
    fn test_total_bytes_refreshed_list_is_consistent() {
        let networks = Networks::new_with_refreshed_list();
        let (rx, tx) = total_bytes(&networks);
        // total_received/total_transmitted are cumulative u64 counters;
        // the only invariant we can assert deterministically is the sum
        // equals manual aggregation over the same snapshot.
        let mut sum_in = 0u64;
        let mut sum_out = 0u64;
        for (_name, data) in networks.iter() {
            sum_in += data.total_received();
            sum_out += data.total_transmitted();
        }
        assert_eq!(rx, sum_in);
        assert_eq!(tx, sum_out);
    }
}
