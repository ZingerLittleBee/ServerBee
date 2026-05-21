//! Security event collectors and detectors.
//!
//! The module is currently Linux-only — most submodules are gated behind
//! `#[cfg(target_os = "linux")]`. On other platforms only the type definitions
//! that are shared with the rest of the agent are exposed, so the binary
//! continues to compile but no detection runs.

#![allow(dead_code)]

pub mod first_seen_store;
pub mod scan_detector;
pub mod ssh_detector;
pub mod ssh_parser;

#[allow(unused_imports)]
pub use first_seen_store::FirstSeenStore;
#[allow(unused_imports)]
pub use scan_detector::{ScanDetector, ScanEmit};
#[allow(unused_imports)]
pub use ssh_detector::{DetectorEmit, SshDetector};
#[allow(unused_imports)]
pub use ssh_parser::{AuthAttempt, AuthMethodHint, AuthOutcome, parse_sshd_line};
