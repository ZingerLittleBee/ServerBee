// ChatGPT unlock detector.
//
// Detection method (independently implemented):
//   OpenAI provides a Cloudflare edge trace endpoint that returns the visitor's
//   geographic location. The `loc=` field contains a two-letter ISO country code.
//   OpenAI operates ChatGPT in most countries but blocks access from a known
//   list of sanctioned/restricted countries:
//     Cuba (CU), Iran (IR), North Korea (KP), Syria (SY), Russia (RU — partial),
//     China (CN — mainland), and others subject to OFAC/EAR restrictions.
//
//   Additionally, ChatGPT's chat interface returns HTTP 403 or redirects to an
//   "access denied" page when accessed from a blocked country.
//
//   Two-step approach:
//   1. Fetch https://chat.openai.com/cdn-cgi/trace — parse the `loc=XX` field.
//      If the country code is in the blocked list => `Blocked`.
//   2. If country not in blocked list => `Unlocked`.
//   3. If trace endpoint fails, fall back to probing
//      https://chat.openai.com/ and checking for 403 or "not available" body.
//
// References consulted (for factual endpoint information only; no code copied):
//   - Cloudflare `cdn-cgi/trace` endpoint: publicly documented Cloudflare feature.
//   - OpenAI usage policies listing restricted countries (https://openai.com/policies/usage-policies).
//
// This file is part of the ServerBee project (AGPL-3.0).

use serverbee_common::protocol::UnlockStatus;

use crate::ip_quality::rule_engine::HttpOutcome;

/// URL for the Cloudflare edge trace (provides `loc=XX`).
pub const TRACE_URL: &str = "https://chat.openai.com/cdn-cgi/trace";

/// Fallback URL for direct ChatGPT access check.
pub const FALLBACK_URL: &str = "https://chat.openai.com/";

/// Timeout for each ChatGPT probe (ms).
pub const TIMEOUT_MS: u32 = 15_000;

/// Two-letter ISO country codes for which OpenAI blocks ChatGPT access.
/// Source: OpenAI usage policies and OFAC/EAR sanction lists (factual data).
const BLOCKED_COUNTRIES: &[&str] = &[
    "CN", // China (mainland) — ChatGPT not available; Baidu controls access
    "CU", // Cuba — OFAC sanctions
    "IR", // Iran — OFAC sanctions
    "KP", // North Korea — OFAC sanctions
    "SY", // Syria — OFAC sanctions
    "RU", // Russia — access restricted/blocked in most regions post-2022
    "BY", // Belarus — partial restrictions
    "VE", // Venezuela — partial OFAC restrictions
    "AF", // Afghanistan — partial restrictions
    "MM", // Myanmar — partial restrictions
];

/// Classify a ChatGPT trace-endpoint response.
///
/// The trace body format is:
/// ```text
/// fl=...
/// h=chat.openai.com
/// ip=1.2.3.4
/// ts=...
/// visit_scheme=https
/// uag=...
/// colo=SJC
/// sliver=...
/// http=http/2
/// loc=US
/// tls=...
/// sni=...
/// warp=off
/// gateway=off
/// rbi=off
/// kex=...
/// ```
pub fn classify_trace(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status != 200 {
        return (UnlockStatus::Failed, None);
    }

    // Parse `loc=XX` from the response body.
    let country = parse_loc_field(&outcome.body);

    match country {
        Some(ref cc) if BLOCKED_COUNTRIES.contains(&cc.as_str()) => {
            (UnlockStatus::Blocked, Some(cc.clone()))
        }
        Some(ref cc) => (UnlockStatus::Unlocked, Some(cc.clone())),
        None => {
            // If we got a 200 but couldn't parse loc=, treat as unknown/failed.
            (UnlockStatus::Failed, None)
        }
    }
}

/// Classify a ChatGPT fallback (homepage) response.
pub fn classify_fallback(outcome: &HttpOutcome) -> (UnlockStatus, Option<String>) {
    if outcome.status == 403 || outcome.status == 451 {
        return (UnlockStatus::Blocked, None);
    }

    let body_lower = outcome.body.to_lowercase();
    if body_lower.contains("access denied")
        || body_lower.contains("not available in your country")
        || body_lower.contains("openai is not available")
    {
        return (UnlockStatus::Blocked, None);
    }

    if outcome.status == 200
        && (body_lower.contains("chatgpt")
            || body_lower.contains("openai")
            || body_lower.contains("sign in")
            || body_lower.contains("log in"))
    {
        return (UnlockStatus::Unlocked, None);
    }

    // A successful HTTP response that matched no positive or negative signal
    // is inconclusive — not evidence of a geo-block.
    (UnlockStatus::Failed, None)
}

/// Parse the `loc=XX` field from a Cloudflare edge trace body.
fn parse_loc_field(body: &str) -> Option<String> {
    for line in body.lines() {
        if let Some(cc) = line.strip_prefix("loc=") {
            let cc = cc.trim().to_uppercase();
            if cc.len() == 2 && cc.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(cc);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ip_quality::rule_engine::HttpOutcome;

    fn trace_outcome(status: u16, body: &str) -> HttpOutcome {
        HttpOutcome {
            status,
            body: body.to_string(),
            final_url: "https://chat.openai.com/cdn-cgi/trace".to_string(),
            redirects: vec![],
        }
    }

    fn fallback_outcome(status: u16, body: &str) -> HttpOutcome {
        HttpOutcome {
            status,
            body: body.to_string(),
            final_url: "https://chat.openai.com/".to_string(),
            redirects: vec![],
        }
    }

    const US_TRACE: &str = "fl=123\nh=chat.openai.com\nip=1.2.3.4\nts=1234\nloc=US\ntls=TLSv1.3\n";
    const CN_TRACE: &str = "fl=123\nh=chat.openai.com\nip=1.2.3.4\nts=1234\nloc=CN\ntls=TLSv1.3\n";
    const IR_TRACE: &str = "fl=123\nh=chat.openai.com\nip=1.2.3.4\nts=1234\nloc=IR\ntls=TLSv1.3\n";
    const KP_TRACE: &str = "fl=123\nh=chat.openai.com\nip=1.2.3.4\nts=1234\nloc=KP\ntls=TLSv1.3\n";

    // ── classify_trace ────────────────────────────────────────────────────────

    #[test]
    fn trace_unlocked_for_us() {
        let (status, region) = classify_trace(&trace_outcome(200, US_TRACE));
        assert_eq!(status, UnlockStatus::Unlocked);
        assert_eq!(region.as_deref(), Some("US"));
    }

    #[test]
    fn trace_blocked_for_china() {
        let (status, region) = classify_trace(&trace_outcome(200, CN_TRACE));
        assert_eq!(status, UnlockStatus::Blocked);
        assert_eq!(region.as_deref(), Some("CN"));
    }

    #[test]
    fn trace_blocked_for_iran() {
        let (status, region) = classify_trace(&trace_outcome(200, IR_TRACE));
        assert_eq!(status, UnlockStatus::Blocked);
        assert_eq!(region.as_deref(), Some("IR"));
    }

    #[test]
    fn trace_blocked_for_north_korea() {
        let (status, region) = classify_trace(&trace_outcome(200, KP_TRACE));
        assert_eq!(status, UnlockStatus::Blocked);
        assert_eq!(region.as_deref(), Some("KP"));
    }

    #[test]
    fn trace_failed_when_non_200() {
        let (status, _) = classify_trace(&trace_outcome(403, ""));
        assert_eq!(status, UnlockStatus::Failed);
    }

    #[test]
    fn trace_failed_when_no_loc_field() {
        let (status, _) = classify_trace(&trace_outcome(200, "fl=123\nip=1.2.3.4\n"));
        assert_eq!(status, UnlockStatus::Failed);
    }

    #[test]
    fn parse_loc_lowercase_normalised() {
        // The trace body might return lowercase; we normalise to uppercase.
        let body = "loc=gb\nfl=123\n";
        let result = parse_loc_field(body);
        assert_eq!(result.as_deref(), Some("GB"));
    }

    // ── classify_fallback ─────────────────────────────────────────────────────

    #[test]
    fn fallback_unlocked_when_200_with_chatgpt() {
        let o = fallback_outcome(200, "<html>ChatGPT — Sign in to OpenAI</html>");
        let (status, _) = classify_fallback(&o);
        assert_eq!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn fallback_blocked_when_403() {
        let (status, _) = classify_fallback(&fallback_outcome(403, ""));
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn fallback_blocked_when_access_denied_body() {
        let o = fallback_outcome(200, "Access denied. OpenAI is not available in your country.");
        let (status, _) = classify_fallback(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn fallback_failed_when_200_no_service_markers() {
        // Ambiguous 200 with no ChatGPT markers => Failed (inconclusive),
        // not Blocked — an unrecognized 200 is not a geo-block signal.
        let o = fallback_outcome(200, "<html>Loading</html>");
        let (status, _) = classify_fallback(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }
}
