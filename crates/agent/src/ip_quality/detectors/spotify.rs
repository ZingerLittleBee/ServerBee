// Spotify unlock detector.
//
// Detection method (independently implemented):
//   Spotify is available in 180+ countries. The service is not available in:
//   CN (China), CU (Cuba), IR (Iran), KP (North Korea), SY (Syria), and a
//   small number of other restricted markets.
//
//   Probe:
//     GET https://open.spotify.com/
//
//   Signals:
//   - HTTP 200 + body contains Spotify-specific content ("Spotify", "Listen",
//     "Sign up", "Premium") => Unlocked
//   - HTTP 200 + body contains unavailability markers => Blocked
//   - HTTP 403/451 => Blocked
//   - Redirect to a "not available" page => Blocked
//
//   Note: Spotify redirects to regional subdomains or login pages in available
//   regions; these are still considered Unlocked. The open.spotify.com endpoint
//   is the main consumer-facing entry point.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Spotify availability list: https://support.spotify.com/us/article/availability-on-spotify/
//   - Publicly observable open.spotify.com behavior.
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Spotify web player / homepage probe.
pub const PROBE_URL: &str = "https://open.spotify.com/";

/// Timeout for the Spotify probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a Spotify probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    // Check redirect chain for unavailability signals.
    let all_urls: Vec<&str> = std::iter::once(outcome.final_url.as_str())
        .chain(outcome.redirects.iter().map(String::as_str))
        .collect();
    for url in &all_urls {
        let url_lower = url.to_lowercase();
        if url_lower.contains("not-available")
            || url_lower.contains("unavailable")
        {
            return (UnlockStatus::Blocked, None);
        }
    }

    let body_lower = outcome.body.to_lowercase();

    if body_lower.contains("not available in your country")
        || body_lower.contains("not available in your region")
        || body_lower.contains("spotify is not available")
        || body_lower.contains("spotify isn't available")
    {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 200
        && (body_lower.contains("spotify")
            || body_lower.contains("listen")
            || body_lower.contains("sign up")
            || body_lower.contains("premium"))
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
    fn unlocked_when_spotify_homepage_200() {
        let o = outcome(
            200,
            "<html>Spotify — Listen to music. Sign up for Premium.</html>",
            "https://open.spotify.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_spotify_body_has_listen() {
        let o = outcome(
            200,
            "Spotify. Listen free or get Premium.",
            "https://open.spotify.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_body_contains_not_available() {
        let o = outcome(
            200,
            "Spotify is not available in your country.",
            "https://open.spotify.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_spotify_isnt_available() {
        let o = outcome(
            200,
            "Spotify isn't available in this region.",
            "https://open.spotify.com/",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_403() {
        let o = outcome(403, "", "https://open.spotify.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_451() {
        let o = outcome(451, "", "https://open.spotify.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirect_to_unavailable() {
        let o = outcome(
            200,
            "page",
            "https://open.spotify.com/unavailable",
            vec!["https://open.spotify.com/"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn failed_when_200_no_service_markers() {
        // Ambiguous 200 with no Spotify markers => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = outcome(200, "<html>Loading</html>", "https://open.spotify.com/", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
