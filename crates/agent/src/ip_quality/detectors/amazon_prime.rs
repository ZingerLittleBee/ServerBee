// Amazon Prime Video unlock detector.
//
// Detection method (independently implemented):
//   Amazon Prime Video uses regional licensing. The Prime Video homepage
//   redirects differently based on region; unavailable regions are redirected
//   to amazon.com/gp/video/splash or similar "not available" pages.
//
//   Probe:
//     GET https://www.primevideo.com/
//
//   Signals:
//   - Final URL remains on primevideo.com and body contains video catalog
//     content ("Watch", "included with Prime", title grid, etc.) => Unlocked
//   - Redirect to amazon.com (not primevideo.com) => potentially available but
//     different region store (still Unlocked in many cases)
//   - Body or final URL contains "not available" or "not supported" => Blocked
//   - HTTP 403/451 => Blocked
//   - Final URL redirects to a /splash or /gp/video/splash page => Blocked
//
//   Note: Amazon Prime Video has broad availability but some regions see
//   content restrictions. We treat reaching the Prime Video catalog as Unlocked
//   since the service is accessible; content library differences are not
//   detectable at the HTTP probe level.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Publicly observable primevideo.com redirect behavior across regions.
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Amazon Prime Video homepage probe.
pub const PROBE_URL: &str = "https://www.primevideo.com/";

/// Timeout for the Amazon Prime Video probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify an Amazon Prime Video probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    let final_url_lower = outcome.final_url.to_lowercase();
    let body_lower = outcome.body.to_lowercase();

    // Redirect to a splash/not-available page signals Blocked.
    if final_url_lower.contains("/splash")
        || final_url_lower.contains("not-available")
        || final_url_lower.contains("not_available")
    {
        return (UnlockStatus::Blocked, None);
    }

    if body_lower.contains("not available in your region")
        || body_lower.contains("not available in your country")
        || body_lower.contains("prime video is not available")
    {
        return (UnlockStatus::Blocked, None);
    }

    // Successfully reached Prime Video catalog.
    if outcome.status == 200
        && (final_url_lower.contains("primevideo.com")
            || final_url_lower.contains("amazon.com/gp/video"))
        && (body_lower.contains("prime")
            || body_lower.contains("watch")
            || body_lower.contains("video"))
    {
        return (UnlockStatus::Unlocked, None);
    }

    // A successful HTTP response that matched no positive or negative signal
    // is inconclusive — not evidence of a geo-block.
    (UnlockStatus::Failed, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ip_quality::rule_engine::HttpOutcome;

    fn outcome(status: u16, body: &str, final_url: &str, redirects: Vec<&str>) -> HttpOutcome {
        HttpOutcome {
            status,
            body: body.to_string(),
            final_url: final_url.to_string(),
            redirects: redirects.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn unlocked_when_primevideo_catalog_returns_200() {
        let o = outcome(
            200,
            "<html>Watch movies and TV with Prime Video</html>",
            "https://www.primevideo.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_redirected_to_amazon_prime_video_path() {
        let o = outcome(
            200,
            "Prime Video Watch now",
            "https://www.amazon.com/gp/video/storefront",
            vec!["https://www.primevideo.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_redirected_to_splash_page() {
        let o = outcome(
            200,
            "Amazon Prime Video",
            "https://www.amazon.com/gp/video/splash",
            vec!["https://www.primevideo.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_body_contains_not_available_in_region() {
        let o = outcome(
            200,
            "Prime Video is not available in your region.",
            "https://www.primevideo.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://www.primevideo.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn failed_when_no_service_content() {
        // Ambiguous 200 with no catalog content => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = outcome(200, "<html>Loading</html>", "https://www.primevideo.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
