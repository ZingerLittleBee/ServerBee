// Google Gemini unlock detector.
//
// Detection method (independently implemented):
//   Google Gemini (formerly Bard) is available in most countries except those
//   under OFAC sanctions and certain markets where Google AI services are
//   restricted. The service uses Google's infrastructure and can be detected via
//   the same Cloudflare-style trace endpoint or by probing the Gemini web app.
//
//   Probe:
//     GET https://gemini.google.com/
//
//   Signals:
//   - HTTP 200 + body contains Gemini-specific content ("Gemini", "Bard",
//     "Google AI", "Try Gemini") => Unlocked
//   - HTTP 200 + body or final URL contains region-block indicators => Blocked
//   - HTTP 403/451 => Blocked
//   - Redirect to accounts.google.com login page => typically Unlocked
//     (just requires auth, service is accessible)
//   - Redirect to an unsupported page => Blocked
//
//   Country blocking:
//   Similar to other Google AI services, Gemini is not available in:
//   CN (China), CU (Cuba), IR (Iran), KP (North Korea), SY (Syria),
//   and a few other restricted regions.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Google Gemini availability page: https://support.google.com/gemini/answer/13278668
//   - Publicly observable gemini.google.com behavior.
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Google Gemini web app probe.
pub const PROBE_URL: &str = "https://gemini.google.com/";

/// Timeout for the Gemini probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a Google Gemini probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    let final_url_lower = outcome.final_url.to_lowercase();
    let body_lower = outcome.body.to_lowercase();

    // Check redirect chain for block indicators.
    let all_urls: Vec<&str> = std::iter::once(outcome.final_url.as_str())
        .chain(outcome.redirects.iter().map(String::as_str))
        .collect();
    for url in &all_urls {
        let url_lower = url.to_lowercase();
        if url_lower.contains("not-available")
            || url_lower.contains("unavailable")
            || url_lower.contains("unsupported-region")
        {
            return (UnlockStatus::Blocked, None);
        }
    }

    if body_lower.contains("not available in your country")
        || body_lower.contains("not available in your region")
        || body_lower.contains("gemini is not available")
        || body_lower.contains("unavailable in your region")
    {
        return (UnlockStatus::Blocked, None);
    }

    // Redirect to Google accounts login means the service is reachable but
    // requires authentication — that is an Unlocked state.
    if final_url_lower.contains("accounts.google.com") {
        return (UnlockStatus::Unlocked, None);
    }

    // Successful Gemini page load.
    if outcome.status == 200
        && (body_lower.contains("gemini")
            || body_lower.contains("google ai")
            || body_lower.contains("bard"))
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
    fn unlocked_when_gemini_page_200() {
        let o = outcome(
            200,
            "<html>Try Gemini — Google AI</html>",
            "https://gemini.google.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_redirected_to_google_accounts() {
        // Redirect to accounts.google.com = service is reachable, needs auth.
        let o = outcome(
            200,
            "<html>Google Accounts</html>",
            "https://accounts.google.com/signin/v2/...",
            vec!["https://gemini.google.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_not_available_in_country() {
        let o = outcome(
            200,
            "Gemini is not available in your country.",
            "https://gemini.google.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://gemini.google.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_451() {
        let o = outcome(451, "", "https://gemini.google.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_unsupported_region() {
        let o = outcome(
            200,
            "page",
            "https://gemini.google.com/unsupported-region",
            vec!["https://gemini.google.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn failed_when_200_no_service_markers() {
        // Ambiguous 200 with no Gemini markers => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = outcome(200, "<html>Loading</html>", "https://gemini.google.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
