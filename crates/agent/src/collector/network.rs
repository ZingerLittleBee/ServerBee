use sysinfo::Networks;

pub fn total_bytes(networks: &Networks) -> (u64, u64) {
    let (mut total_in, mut total_out) = (0u64, 0u64);
    for (_name, data) in networks.iter() {
        total_in += data.total_received();
        total_out += data.total_transmitted();
    }
    (total_in, total_out)
}
