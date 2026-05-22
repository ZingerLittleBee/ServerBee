// TikTok unlock detector.
//
// Detection method (independently implemented):
//   TikTok is unavailable in India (banned since 2020), partially restricted in
//   the United States (contested legislation), and not available in other regions
//   (CN uses Douyin instead). The main TikTok webapp at tiktok.com shows
//   region-specific availability.
//
//   Probe:
//     GET https://www.tiktok.com/
//
//   Signals:
//   - HTTP 200 + body contains TikTok-specific content ("TikTok", "Make Your Day",
//     "For You", "Discover", "Sign up") => Unlocked
//   - HTTP 200 + body or final URL contains block/ban indicators => Blocked
//   - HTTP 403/451 => Blocked
//   - Redirect to a regional ban notice or unavailability page => Blocked
//   - Body contains "tiktok is not available" or regional ban text => Blocked
//
//   Note: In China, tiktok.com redirects to douyin.com or shows a "not available"
//   page. India's ISPs block the domain entirely (DNS/TCP block, not HTTP-level),
//   which would result in a `Failed` status at the network layer rather than here.
//   The classify function handles HTTP-level observable signals.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Publicly observable tiktok.com behavior across regions.
//   - TikTok's regional availability and India ban (factual knowledge).
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the TikTok homepage probe.
pub const PROBE_URL: &str = "https://www.tiktok.com/";

/// Timeout for the TikTok probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a TikTok probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    // Check redirect chain for block indicators.
    let all_urls: Vec<&str> = std::iter::once(outcome.final_url.as_str())
        .chain(outcome.redirects.iter().map(String::as_str))
        .collect();
    for url in &all_urls {
        let url_lower = url.to_lowercase();
        if url_lower.contains("not-available")
            || url_lower.contains("unavailable")
            // Redirect to douyin.com signals CN region (TikTok not available).
            || url_lower.contains("douyin.com")
        {
            return (UnlockStatus::Blocked, None);
        }
    }

    let body_lower = outcome.body.to_lowercase();

    if body_lower.contains("tiktok is not available in your region")
        || body_lower.contains("tiktok is not available in your country")
        || body_lower.contains("not available in your region")
        || body_lower.contains("this service is not available")
        // Douyin-redirect body signals CN block.
        || body_lower.contains("douyin")
    {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 200
        && (body_lower.contains("tiktok")
            || body_lower.contains("make your day")
            || body_lower.contains("for you")
            || body_lower.contains("discover")
            || body_lower.contains("sign up"))
    {
        return (UnlockStatus::Unlocked, None);
    }

    // Fail closed.
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
    fn unlocked_when_tiktok_homepage_200() {
        let o = outcome(
            200,
            "<html>TikTok — Make Your Day. Sign up to see videos.</html>",
            "https://www.tiktok.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_tiktok_for_you_body() {
        let o = outcome(
            200,
            "TikTok | For You",
            "https://www.tiktok.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_not_available_in_region() {
        let o = outcome(
            200,
            "TikTok is not available in your region.",
            "https://www.tiktok.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_body_contains_douyin() {
        // China: TikTok redirects users to Douyin.
        let o = outcome(
            200,
            "Please visit douyin.com for the Chinese market.",
            "https://www.tiktok.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_douyin() {
        let o = outcome(
            200,
            "<html>Douyin</html>",
            "https://www.douyin.com/",
            vec!["https://www.tiktok.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://www.tiktok.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_451() {
        let o = outcome(451, "", "https://www.tiktok.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_not_available() {
        let o = outcome(
            200,
            "page",
            "https://www.tiktok.com/not-available",
            vec!["https://www.tiktok.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_200_no_service_markers() {
        let o = outcome(200, "<html>Loading</html>", "https://www.tiktok.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }
}
