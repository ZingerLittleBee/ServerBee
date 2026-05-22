// Netflix unlock detector.
//
// Detection method (independently implemented):
//   Netflix region-gates content at the title level. Requesting a non-original
//   title page returns HTTP 200 in unlocked regions and HTTP 404/403 elsewhere.
//   Requesting a self-produced original (which is available everywhere Netflix
//   operates) from a region with only "originals-only" access also returns 200.
//
//   Two probes are issued:
//   1. Non-original title — title ID 81280792 ("Breaking Bad").
//      200 => fully unlocked region
//      404/403/other => check originals probe
//   2. Netflix original — title ID 80018499 ("Stranger Things", a Netflix
//      original available in more regions).
//      If probe 1 is blocked but probe 2 returns 200 => originals-only (Restricted).
//      If both blocked => Blocked.
//
// References consulted (for factual endpoint information only; no code copied):
//   - netflix-verify (MIT): https://github.com/sjlleo/netflix-verify
//   - Motivation: observing Netflix's public HTTP behavior (title 404 pattern)
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for a non-Netflix-original title (Breaking Bad, id 81280792).
pub const NON_ORIGINAL_URL: &str = "https://www.netflix.com/title/81280792";

/// URL for a Netflix original (Stranger Things, id 80018499).
pub const ORIGINAL_URL: &str = "https://www.netflix.com/title/80018499";

/// Timeout for each Netflix probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify the pair of Netflix responses:
/// - `non_orig`: outcome for the non-original title probe.
/// - `orig`:     outcome for the Netflix-original title probe.
///
/// Returns `(UnlockStatus, Option<region_string>)`.
/// Region detection from Netflix requires a separate geo API call; we return
/// `None` here and let the caller detect region via the URL or body if needed.
///
/// Decision table:
/// | non_orig status | orig status | result     |
/// |-----------------|-------------|------------|
/// | 200             | *           | Unlocked   |
/// | non-200         | 200         | Restricted |
/// | non-200         | non-200     | Blocked    |
pub fn classify(non_orig: &HttpOutcome, orig: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    let status = if non_orig.status == 200 {
        UnlockStatus::Unlocked
    } else if orig.status == 200 {
        UnlockStatus::Restricted
    } else {
        UnlockStatus::Blocked
    };
    (status, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ip_quality::rule_engine::HttpOutcome;

    fn outcome(status: u16) -> HttpOutcome {
        HttpOutcome {
            status,
            body: String::new(),
            final_url: "https://www.netflix.com/".to_string(),
            redirects: vec![],
        }
    }

    #[test]
    fn fully_unlocked_when_non_original_returns_200() {
        let (status, region) = classify(&outcome(200), &outcome(200));
        assert_eq!(status, UnlockStatus::Unlocked);
        assert!(region.is_none());
    }

    #[test]
    fn fully_unlocked_even_if_original_is_404() {
        // Non-original accessible => full unlock regardless of original status.
        let (status, _) = classify(&outcome(200), &outcome(404));
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn originals_only_when_non_original_blocked_but_original_200() {
        let (status, _) = classify(&outcome(404), &outcome(200));
        assert_eq!(status, UnlockStatus::Restricted);
    }

    #[test]
    fn originals_only_when_non_original_403_but_original_200() {
        let (status, _) = classify(&outcome(403), &outcome(200));
        assert_eq!(status, UnlockStatus::Restricted);
    }

    #[test]
    fn blocked_when_both_non_200() {
        let (status, _) = classify(&outcome(404), &outcome(404));
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_both_403() {
        let (status, _) = classify(&outcome(403), &outcome(403));
        assert_eq!(status, UnlockStatus::Blocked);
    }
}
