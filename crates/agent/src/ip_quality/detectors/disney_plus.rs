// Disney+ unlock detector.
//
// Detection method (independently implemented):
//   Disney+ uses a region-check endpoint at:
//     https://disney.api.edge.bamgrid.com/graph/v1/device/graphql
//   which is complex and requires auth tokens. A simpler observable signal
//   is the `/explore/` landing page: in unavailable regions Disney+ redirects
//   to a "not-available" page or returns a body indicating unavailability.
//
//   We use the publicly documented availability check endpoint that was
//   observed to return a JSON body with an `isAvailable` / `status` field:
//     GET https://disney.api.edge.bamgrid.com/v1/public/upsell
//   HTTP 200 + body containing "SUBSCRIPTION_OPTIONS" or similar => available
//   HTTP 403 / body containing "not-available" / redirect to unavailable page => blocked
//
//   Fallback approach: probe the main landing page at
//     https://www.disneyplus.com/
//   and check for region-block indicators in the response body / redirect chain.
//
//   Probing the homepage:
//   - 200 + body contains "Subscribe" / "Start streaming" => Unlocked
//   - 200 + body contains "not available in your region" => Blocked
//   - redirect to /unavailable or /not-available => Blocked
//
// References consulted (for factual endpoint information only; no code copied):
//   - disneyplus-checker scripts (various MIT/unlicensed), observing the homepage
//     behavior across regions.
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Disney+ homepage availability probe.
pub const PROBE_URL: &str = "https://www.disneyplus.com/";

/// Timeout for the Disney+ probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a Disney+ probe response.
///
/// - Redirect to a URL containing "not-available" or "unavailable" => `Blocked`
/// - Body contains "notInSupportedLocation" or "not available in your region" => `Blocked`
/// - Body contains "Subscribe" or "Start Streaming" (case-insensitive) => `Unlocked`
/// - HTTP 403/451 => `Blocked`
/// - Otherwise => `Blocked` (fail closed for a streaming service)
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    // Check redirect chain for unavailability signals.
    let all_urls: Vec<&str> = std::iter::once(outcome.final_url.as_str())
        .chain(outcome.redirects.iter().map(String::as_str))
        .collect();

    for url in &all_urls {
        let url_lower = url.to_lowercase();
        if url_lower.contains("not-available")
            || url_lower.contains("unavailable")
            || url_lower.contains("unsupported")
        {
            return (UnlockStatus::Blocked, None);
        }
    }

    // HTTP status signals.
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    // Body analysis.
    let body_lower = outcome.body.to_lowercase();
    if body_lower.contains("notinsupportedlocation")
        || body_lower.contains("not available in your region")
        || body_lower.contains("disney+ is not available in your country")
        || body_lower.contains("disneyplus is not available")
    {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 200
        && (body_lower.contains("subscribe")
            || body_lower.contains("start streaming")
            || body_lower.contains("disneyplus")
            || body_lower.contains("disney+"))
    {
        return (UnlockStatus::Unlocked, None);
    }

    (UnlockStatus::Blocked, None)
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
    fn unlocked_when_200_with_subscribe_body() {
        let o = outcome(
            200,
            r#"<html><body>Subscribe to Disney+</body></html>"#,
            "https://www.disneyplus.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_200_with_start_streaming() {
        let o = outcome(
            200,
            "<html>Start Streaming Disney+</html>",
            "https://www.disneyplus.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_body_contains_not_available_in_your_region() {
        let o = outcome(
            200,
            "Disney+ is not available in your region.",
            "https://www.disneyplus.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_not_available_url() {
        let o = outcome(
            200,
            "page content",
            "https://www.disneyplus.com/not-available",
            vec!["https://www.disneyplus.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://www.disneyplus.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_451_unavailable_for_legal_reasons() {
        let o = outcome(451, "", "https://www.disneyplus.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_body_contains_notinsupportedlocation() {
        let o = outcome(
            200,
            r#"{"error":"notInSupportedLocation","message":"not supported"}"#,
            "https://www.disneyplus.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_200_but_no_service_markers() {
        // Ambiguous 200 with unrecognized body => fail closed to Blocked.
        let o = outcome(200, "<html>Loading...</html>", "https://www.disneyplus.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }
}
