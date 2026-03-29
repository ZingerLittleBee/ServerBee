use sha2::{Digest, Sha256};

/// Generate a machine fingerprint: SHA-256 of "{hostname}:{machine_id}".
/// Returns empty string if machine_id is unavailable (caller should skip fingerprint).
pub fn generate() -> String {
    let machine_id = match read_machine_id() {
        Some(id) => id,
        None => {
            tracing::warn!("Could not read machine-id, fingerprint will be skipped");
            return String::new();
        }
    };

    let hostname = gethostname::gethostname()
        .to_string_lossy()
        .to_string();

    let input = format!("{hostname}:{machine_id}");
    let hash = Sha256::digest(input.as_bytes());
    hex::encode(hash)
}

#[cfg(target_os = "linux")]
fn read_machine_id() -> Option<String> {
    std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
fn read_machine_id() -> Option<String> {
    let output = std::process::Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("IOPlatformUUID") {
            return line.split('"').nth(3).map(|s| s.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn read_machine_id() -> Option<String> {
    let output = std::process::Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("MachineGuid") {
            return line.split_whitespace().last().map(|s| s.to_string());
        }
    }
    None
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_machine_id() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_returns_64_hex_chars_or_empty() {
        let fp = generate();
        assert!(
            fp.is_empty() || (fp.len() == 64 && fp.chars().all(|c| c.is_ascii_hexdigit())),
            "Fingerprint must be empty or 64 hex chars, got: {fp}"
        );
    }

    #[test]
    fn test_sha256_deterministic() {
        // Same input always produces the same hash
        let input = "testhost:test-machine-id-1234";
        let hash1 = hex::encode(Sha256::digest(input.as_bytes()));
        let hash2 = hex::encode(Sha256::digest(input.as_bytes()));
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }
}
