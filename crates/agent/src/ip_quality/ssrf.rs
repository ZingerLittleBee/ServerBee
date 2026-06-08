//! SSRF guard.
//!
//! The implementation moved to `serverbee_common::ssrf` so the server-side
//! service-monitor checkers can reuse the exact same validation. It is
//! re-exported here so existing `super::ssrf::*` references keep working and the
//! agent retains a single, shared source of truth.
pub use serverbee_common::ssrf::*;
