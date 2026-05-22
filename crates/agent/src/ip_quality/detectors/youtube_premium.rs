// YouTube Premium unlock detector.
//
// Detection method (independently implemented):
//   YouTube Premium availability is determined by checking the premium signup
//   landing page. When Premium is available in a region, the page renders with
//   plan/pricing information. When unavailable, the page either redirects to
//   a generic YouTube page or contains text indicating Premium is not offered
//   in the current country.
//
//   Probe:
//     GET https://www.youtube.com/premium
//
//   Signals:
//   - HTTP 200 + body contains "YouTube Premium" and pricing/plan content
//     ("per month", "₹", "$", "€", "plan", "subscribe") => Unlocked
//   - HTTP 200 + body contains "not available in your country" => Blocked
//   - Redirect away from /premium (to youtube.com home) => Blocked
//   - HTTP 404 / 403 => Blocked
//
// References consulted (for factual endpoint information only; no code copied):
//   - Publicly observable YouTube Premium page behavior (personal observation).
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the YouTube Premium signup page.
pub const PROBE_URL: &str = "https://www.youtube.com/premium";

/// Timeout for the YouTube Premium probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Classify a YouTube Premium probe response.
pub fn classify(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    // If the final URL no longer contains "premium", we were redirected away.
    if !outcome.final_url.contains("premium") && !outcome.redirects.is_empty() {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 404 || outcome.status == 403 {
        return (UnlockStatus::Blocked, None);
    }

    let body_lower = outcome.body.to_lowercase();

    if body_lower.contains("not available in your country")
        || body_lower.contains("not available in your region")
        || body_lower.contains("youtube premium is not available")
    {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 200
        && body_lower.contains("youtube premium")
        && (body_lower.contains("per month")
            || body_lower.contains("plan")
            || body_lower.contains("subscribe")
            || body_lower.contains("free trial")
            || body_lower.contains("get youtube premium"))
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
    fn unlocked_when_premium_page_with_plan_info() {
        let o = outcome(
            200,
            "<html>YouTube Premium — Get YouTube Premium for $13.99 per month. Subscribe now.</html>",
            "https://www.youtube.com/premium",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn unlocked_when_premium_page_with_free_trial() {
        let o = outcome(
            200,
            "YouTube Premium — Start a free trial today.",
            "https://www.youtube.com/premium",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn blocked_when_not_available_in_country() {
        let o = outcome(
            200,
            "YouTube Premium is not available in your country.",
            "https://www.youtube.com/premium",
            vec![],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_redirected_away_from_premium() {
        let o = outcome(
            200,
            "<html>YouTube</html>",
            "https://www.youtube.com/",
            vec!["https://www.youtube.com/premium"],
        );
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn blocked_when_404() {
        let o = outcome(404, "Not found", "https://www.youtube.com/premium", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn failed_when_200_no_plan_signals() {
        // Ambiguous 200 with no plan signals => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = outcome(200, "<html>Loading...</html>", "https://www.youtube.com/premium", vec![]);
        let (status, _) = classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
