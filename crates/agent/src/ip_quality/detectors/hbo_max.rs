// HBO Max (Max) unlock detector.
//
// Detection method (independently implemented):
//   HBO Max (now branded as "Max") uses regional licensing and restricts
//   service to specific countries. The service is primarily available in
//   the United States, Latin America, and select European/Asian markets.
//
//   Probe:
//     GET https://www.max.com/
//
//   Signals:
//   - HTTP 200 + body contains streaming catalog content
//     ("Subscribe", "Start Streaming", "Max Original", "Plans", pricing) => Unlocked
//   - Body or final URL indicates unavailability => Blocked
//   - HTTP 403/451 => Blocked
//   - Redirect to an HBO/Max "not available" page => Blocked
//
//   Note: The domain changed from hbomax.com to max.com in 2023. We probe
//   max.com as the primary URL.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Publicly observable max.com behavior across regions.
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Max (HBO Max) homepage probe.
pub const PROBE_URL: &str = "https://www.max.com/";

/// Timeout for the Max probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a Max (HBO Max) probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    let final_url_lower = outcome.final_url.to_lowercase();
    let body_lower = outcome.body.to_lowercase();

    // Redirect chain or final URL signals unavailability.
    let all_urls: Vec<&str> = std::iter::once(outcome.final_url.as_str())
        .chain(outcome.redirects.iter().map(String::as_str))
        .collect();
    for url in &all_urls {
        let url_lower = url.to_lowercase();
        if url_lower.contains("not-available")
            || url_lower.contains("unavailable")
            || url_lower.contains("not-supported")
        {
            return (UnlockStatus::Blocked, None);
        }
    }

    if body_lower.contains("not available in your region")
        || body_lower.contains("not available in your country")
        || body_lower.contains("max is not available")
        || body_lower.contains("hbo max is not available")
    {
        return (UnlockStatus::Blocked, None);
    }

    // Successful catalog page.
    if outcome.status == 200
        && (final_url_lower.contains("max.com") || final_url_lower.contains("hbomax.com"))
        && (body_lower.contains("subscribe")
            || body_lower.contains("start streaming")
            || body_lower.contains("plans")
            || body_lower.contains("max original")
            || body_lower.contains("hbo"))
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
    fn unlocked_when_max_catalog_200() {
        let o = outcome(
            200,
            "<html>Subscribe to Max. Start streaming HBO Original content.</html>",
            "https://www.max.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_max_plans_page() {
        let o = outcome(
            200,
            "Max — Plans starting at $9.99/mo.",
            "https://www.max.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_not_available_in_region() {
        let o = outcome(
            200,
            "Max is not available in your region.",
            "https://www.max.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://www.max.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_451() {
        let o = outcome(451, "", "https://www.max.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_unavailable_url() {
        let o = outcome(
            200,
            "page",
            "https://www.max.com/unavailable",
            vec!["https://www.max.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn failed_when_200_no_service_markers() {
        // Ambiguous 200 with no catalog markers => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = outcome(200, "<html>Loading</html>", "https://www.max.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
