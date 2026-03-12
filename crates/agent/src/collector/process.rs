use sysinfo::System;

pub fn count(sys: &System) -> i32 {
    sys.processes().len() as i32
}

pub fn tcp_connections() -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines("/proc/net/tcp").unwrap_or(0) + count_lines("/proc/net/tcp6").unwrap_or(0)
    }
    #[cfg(target_os = "windows")]
    {
        count_connections_netstat("TCP")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        0
    }
}

pub fn udp_connections() -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines("/proc/net/udp").unwrap_or(0) + count_lines("/proc/net/udp6").unwrap_or(0)
    }
    #[cfg(target_os = "windows")]
    {
        count_connections_netstat("UDP")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        0
    }
}

#[cfg(target_os = "linux")]
fn count_lines(path: &str) -> std::io::Result<i32> {
    let content = std::fs::read_to_string(path)?;
    Ok((content.lines().count().saturating_sub(1)) as i32)
}

/// On Windows, run `netstat -an -p <protocol>` and count established/active lines.
#[cfg(target_os = "windows")]
fn count_connections_netstat(protocol: &str) -> i32 {
    let output = match std::process::Command::new("netstat")
        .args(["-an", "-p", protocol])
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            tracing::debug!("Failed to run netstat for {protocol}: {e}");
            return 0;
        }
    };

    if !output.status.success() {
        return 0;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Count lines that start with whitespace followed by the protocol name,
    // which indicates actual connection entries (skip header lines).
    stdout
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with(protocol)
        })
        .count() as i32
}
