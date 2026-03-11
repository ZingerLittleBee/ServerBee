use sysinfo::System;

pub fn count(sys: &System) -> i32 {
    sys.processes().len() as i32
}

pub fn tcp_connections() -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines("/proc/net/tcp").unwrap_or(0) + count_lines("/proc/net/tcp6").unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

pub fn udp_connections() -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines("/proc/net/udp").unwrap_or(0) + count_lines("/proc/net/udp6").unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

#[cfg(target_os = "linux")]
fn count_lines(path: &str) -> std::io::Result<i32> {
    let content = std::fs::read_to_string(path)?;
    Ok((content.lines().count().saturating_sub(1)) as i32)
}
