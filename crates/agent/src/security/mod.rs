//! Security event collectors and detectors.
//!
//! Collectors run on Linux. Platform-neutral parsers and detectors remain
//! available for tests on other platforms, where watcher entry points are inert.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

pub mod conntrack_watcher;
pub mod first_seen_store;
pub mod journal_watcher;
pub mod manager;
pub mod scan_detector;
pub mod ssh_detector;
pub mod ssh_parser;

pub use manager::SecurityManager;
