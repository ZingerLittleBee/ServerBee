//! Parser for sshd auth lines emitted via journalctl or `/var/log/auth.log`.
//!
//! Recognises three line shapes that drive brute-force detection and the
//! ssh_login event:
//! * `Accepted <method> for <user> from <ip> port <port> ssh2: ...`
//! * `Failed <method> for [invalid user ]<user> from <ip> port <port> ssh2`
//! * `Invalid user <user> from <ip> port <port>`

use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthOutcome {
    Success { auth_method: AuthMethodHint },
    Failure { invalid_user: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethodHint {
    Publickey,
    Password,
    KeyboardInteractive,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthAttempt {
    pub outcome: AuthOutcome,
    pub username: String,
    pub source_ip: String,
    pub source_port: Option<u16>,
}

static RE_ACCEPTED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Accepted (publickey|password|keyboard-interactive|\S+) for (\S+) from (\S+) port (\d+)")
        .expect("RE_ACCEPTED")
});
static RE_FAILED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Failed \S+ for (invalid user )?(\S+) from (\S+) port (\d+)").expect("RE_FAILED")
});
static RE_INVALID_USER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^Invalid user (\S+) from (\S+) port (\d+)").expect("RE_INVALID_USER")
});

pub fn parse_sshd_line(line: &str) -> Option<AuthAttempt> {
    if let Some(c) = RE_ACCEPTED.captures(line) {
        let method = match &c[1] {
            "publickey" => AuthMethodHint::Publickey,
            "password" => AuthMethodHint::Password,
            "keyboard-interactive" => AuthMethodHint::KeyboardInteractive,
            _ => AuthMethodHint::Other,
        };
        return Some(AuthAttempt {
            outcome: AuthOutcome::Success {
                auth_method: method,
            },
            username: c[2].to_string(),
            source_ip: c[3].to_string(),
            source_port: c[4].parse().ok(),
        });
    }
    if let Some(c) = RE_FAILED.captures(line) {
        let invalid_user = c.get(1).is_some();
        return Some(AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user },
            username: c[2].to_string(),
            source_ip: c[3].to_string(),
            source_port: c[4].parse().ok(),
        });
    }
    if let Some(c) = RE_INVALID_USER.captures(line) {
        return Some(AuthAttempt {
            outcome: AuthOutcome::Failure { invalid_user: true },
            username: c[1].to_string(),
            source_ip: c[2].to_string(),
            source_port: c[3].parse().ok(),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accepted_publickey() {
        let line =
            "Accepted publickey for root from 203.0.113.5 port 12345 ssh2: ED25519 SHA256:abc";
        let a = parse_sshd_line(line).unwrap();
        assert_eq!(a.username, "root");
        assert_eq!(a.source_ip, "203.0.113.5");
        assert_eq!(a.source_port, Some(12345));
        assert!(matches!(
            a.outcome,
            AuthOutcome::Success {
                auth_method: AuthMethodHint::Publickey
            }
        ));
    }

    #[test]
    fn parses_failed_password() {
        let line = "Failed password for root from 198.51.100.7 port 60000 ssh2";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(
            a.outcome,
            AuthOutcome::Failure {
                invalid_user: false
            }
        ));
        assert_eq!(a.source_ip, "198.51.100.7");
    }

    #[test]
    fn parses_failed_invalid_user() {
        let line = "Failed password for invalid user fake from 10.0.0.5 port 50000 ssh2";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(
            a.outcome,
            AuthOutcome::Failure { invalid_user: true }
        ));
        assert_eq!(a.username, "fake");
    }

    #[test]
    fn parses_invalid_user_line() {
        let line = "Invalid user attacker from 192.0.2.5 port 22000";
        let a = parse_sshd_line(line).unwrap();
        assert!(matches!(
            a.outcome,
            AuthOutcome::Failure { invalid_user: true }
        ));
        assert_eq!(a.username, "attacker");
    }

    #[test]
    fn parses_ipv6() {
        let line = "Accepted publickey for ubuntu from 2001:db8::1 port 22 ssh2: RSA SHA256:xyz";
        let a = parse_sshd_line(line).unwrap();
        assert_eq!(a.source_ip, "2001:db8::1");
    }

    #[test]
    fn returns_none_on_unrelated_line() {
        assert!(parse_sshd_line("pam_unix(sshd:session): session opened").is_none());
    }
}
