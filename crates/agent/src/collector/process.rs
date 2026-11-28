#[cfg(target_os = "linux")]
pub fn count() -> i32 {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    let mut total: i32 = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let bytes = name.as_encoded_bytes();
        if !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit) {
            total = total.saturating_add(1);
        }
    }
    total
}

#[cfg(not(target_os = "linux"))]
pub fn count() -> i32 {
    use sysinfo::{ProcessRefreshKind, RefreshKind, System};
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_processes(ProcessRefreshKind::nothing()),
    );
    sys.processes().len() as i32
}

pub fn tcp_connections_with_buf(buf: &mut [u8]) -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines_with_buf("/proc/net/tcp", buf).unwrap_or(0)
            + count_lines_with_buf("/proc/net/tcp6", buf).unwrap_or(0)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = buf;
        count_connections_netstat("TCP")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = buf;
        0
    }
}

pub fn udp_connections_with_buf(buf: &mut [u8]) -> i32 {
    #[cfg(target_os = "linux")]
    {
        count_lines_with_buf("/proc/net/udp", buf).unwrap_or(0)
            + count_lines_with_buf("/proc/net/udp6", buf).unwrap_or(0)
    }
    #[cfg(target_os = "windows")]
    {
        let _ = buf;
        count_connections_netstat("UDP")
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = buf;
        0
    }
}

#[cfg(test)]
pub fn tcp_connections() -> i32 {
    let mut buf = [0u8; 8192];
    tcp_connections_with_buf(&mut buf)
}

#[cfg(test)]
pub fn udp_connections() -> i32 {
    let mut buf = [0u8; 8192];
    udp_connections_with_buf(&mut buf)
}

#[cfg(target_os = "linux")]
fn count_lines_with_buf(path: &str, buf: &mut [u8]) -> std::io::Result<i32> {
    use std::io::Read;

    let mut file = std::fs::File::open(path)?;
    let mut lines: usize = 0;
    let mut last_byte: Option<u8> = None;

    loop {
        let n = file.read(buf)?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        lines += chunk.iter().filter(|&&b| b == b'\n').count();
        last_byte = Some(chunk[n - 1]);
    }

    if let Some(last) = last_byte
        && last != b'\n'
    {
        lines += 1;
    }

    Ok(lines.saturating_sub(1) as i32)
}

#[cfg(all(test, target_os = "linux"))]
fn count_lines(path: &str) -> std::io::Result<i32> {
    let mut buf = [0u8; 8192];
    count_lines_with_buf(path, &mut buf)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_positive() {
        assert!(count() > 0, "at least the test process must be running");
    }

    #[test]
    fn test_tcp_connections_non_negative() {
        // On Linux/Windows this reads real connection tables; on macOS it
        // returns 0. Either way the result must be non-negative.
        assert!(tcp_connections() >= 0);
    }

    #[test]
    fn test_udp_connections_non_negative() {
        assert!(udp_connections() >= 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_count_lines_missing_file_errors() {
        // count_lines propagates an io::Error for a non-existent path.
        assert!(count_lines("/proc/this/does/not/exist").is_err());
    }
}
