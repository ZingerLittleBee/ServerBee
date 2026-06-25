use sha2::{Digest, Sha256};

/// Generate a machine fingerprint from stable host identity.
/// Returns empty string if machine_id is unavailable (caller should skip fingerprint).
pub fn generate() -> String {
    let machine_id = match read_machine_id() {
        Some(id) => id,
        None => {
            tracing::warn!("Could not read machine-id, fingerprint will be skipped");
            return String::new();
        }
    };

    fingerprint_from_machine_id(&machine_id)
}

fn fingerprint_from_machine_id(machine_id: &str) -> String {
    let hash = Sha256::digest(machine_id.as_bytes());
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
    fn test_fingerprint_uses_machine_id_only() {
        let machine_id = "test-machine-id-1234";
        let expected = hex::encode(Sha256::digest(machine_id.as_bytes()));
        assert_eq!(fingerprint_from_machine_id(machine_id), expected);
    }

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
        let machine_id = "test-machine-id-1234";
        let hash1 = fingerprint_from_machine_id(machine_id);
        let hash2 = fingerprint_from_machine_id(machine_id);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn fingerprint_distinct_inputs_yield_distinct_hashes() {
        // Different machine ids must not collide.
        let a = fingerprint_from_machine_id("host-a");
        let b = fingerprint_from_machine_id("host-b");
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(b.len(), 64);
    }

    #[test]
    fn fingerprint_of_empty_input_is_sha256_of_empty() {
        // The helper does not special-case empty input; it hashes verbatim.
        // (The empty-id guard lives in read_machine_id, not here.)
        let expected = hex::encode(Sha256::digest(b""));
        assert_eq!(fingerprint_from_machine_id(""), expected);
        assert_eq!(fingerprint_from_machine_id("").len(), 64);
    }

    #[test]
    fn fingerprint_matches_known_vector() {
        // Lock in a known SHA-256 hex to guard against accidental algorithm
        // changes. echo -n "abc" | sha256sum.
        let known = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(fingerprint_from_machine_id("abc"), known);
    }

    #[test]
    fn generate_is_deterministic_across_calls() {
        // On any host, generate() must be stable: either consistently empty
        // (no machine-id) or a consistent 64-hex digest.
        let first = generate();
        let second = generate();
        assert_eq!(first, second);
    }

    /// On macOS the test runner can read the platform UUID via `ioreg`, so the
    /// `read_machine_id` arm and the non-empty `generate` branch are exercised.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_read_machine_id_yields_non_empty_fingerprint() {
        if let Some(id) = read_machine_id() {
            assert!(!id.is_empty());
            let fp = generate();
            assert_eq!(fp.len(), 64);
            assert_eq!(fp, fingerprint_from_machine_id(&id));
        }
    }
}
