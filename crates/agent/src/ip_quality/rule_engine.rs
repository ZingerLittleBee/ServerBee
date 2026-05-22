// Used by UnlockChecker (later unit).
#![allow(dead_code)]

use regex::Regex;
use serverbee_common::protocol::{UnlockMatch, UnlockRule, UnlockStatus};

/// The outcome of an HTTP fetch, consumed by the rule engine and detectors.
#[derive(Debug, Clone)]
pub struct HttpOutcome {
    pub status: u16,
    pub body: String,
    pub final_url: String,
    pub redirects: Vec<String>,
}

/// Evaluate `rules` against `outcome` in order; return the result of the
/// first matching rule. Returns `UnlockStatus::Failed` when no rule matches.
pub fn apply_rules(outcome: &HttpOutcome, rules: &[UnlockRule]) -> UnlockStatus {
    for rule in rules {
        if matches_rule(outcome, &rule.match_) {
            return rule.result;
        }
    }
    UnlockStatus::Failed
}

fn matches_rule(outcome: &HttpOutcome, m: &UnlockMatch) -> bool {
    match m {
        UnlockMatch::StatusEquals { code } => outcome.status == *code,
        UnlockMatch::StatusInRange { min, max } => outcome.status >= *min && outcome.status <= *max,
        UnlockMatch::BodyRegex { pattern } => {
            Regex::new(pattern).map(|re| re.is_match(&outcome.body)).unwrap_or(false)
        }
        UnlockMatch::RedirectMatches { pattern } => {
            let re = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => return false,
            };
            // Match against the final URL and every intermediate redirect URL
            if re.is_match(&outcome.final_url) {
                return true;
            }
            outcome.redirects.iter().any(|url| re.is_match(url))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serverbee_common::protocol::{UnlockMatch, UnlockRule, UnlockStatus};

    fn outcome(status: u16, body: &str, final_url: &str, redirects: Vec<&str>) -> HttpOutcome {
        HttpOutcome {
            status,
            body: body.to_string(),
            final_url: final_url.to_string(),
            redirects: redirects.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    fn rule(match_: UnlockMatch, result: UnlockStatus) -> UnlockRule {
        UnlockRule { match_, result }
    }

    // ── StatusEquals ─────────────────────────────────────────────────────────

    #[test]
    fn status_equals_matches_exact_code() {
        let rules = vec![rule(UnlockMatch::StatusEquals { code: 200 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(200, "", "http://x.com/", vec![]), &rules), UnlockStatus::Unlocked);
    }

    #[test]
    fn status_equals_no_match_returns_failed() {
        let rules = vec![rule(UnlockMatch::StatusEquals { code: 200 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(404, "", "http://x.com/", vec![]), &rules), UnlockStatus::Failed);
    }

    // ── StatusInRange ─────────────────────────────────────────────────────────

    #[test]
    fn status_in_range_matches_boundary_min() {
        let rules = vec![rule(UnlockMatch::StatusInRange { min: 200, max: 299 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(200, "", "http://x.com/", vec![]), &rules), UnlockStatus::Unlocked);
    }

    #[test]
    fn status_in_range_matches_boundary_max() {
        let rules = vec![rule(UnlockMatch::StatusInRange { min: 200, max: 299 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(299, "", "http://x.com/", vec![]), &rules), UnlockStatus::Unlocked);
    }

    #[test]
    fn status_in_range_no_match_below() {
        let rules = vec![rule(UnlockMatch::StatusInRange { min: 200, max: 299 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(199, "", "http://x.com/", vec![]), &rules), UnlockStatus::Failed);
    }

    #[test]
    fn status_in_range_no_match_above() {
        let rules = vec![rule(UnlockMatch::StatusInRange { min: 200, max: 299 }, UnlockStatus::Unlocked)];
        assert_eq!(apply_rules(&outcome(300, "", "http://x.com/", vec![]), &rules), UnlockStatus::Failed);
    }

    // ── BodyRegex ────────────────────────────────────────────────────────────

    #[test]
    fn body_regex_matches_pattern() {
        let rules = vec![rule(
            UnlockMatch::BodyRegex { pattern: "unavailable in your region".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(
                &outcome(200, "This content is unavailable in your region.", "http://x.com/", vec![]),
                &rules
            ),
            UnlockStatus::Blocked
        );
    }

    #[test]
    fn body_regex_no_match() {
        let rules = vec![rule(
            UnlockMatch::BodyRegex { pattern: "unavailable in your region".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(&outcome(200, "Welcome!", "http://x.com/", vec![]), &rules),
            UnlockStatus::Failed
        );
    }

    #[test]
    fn body_regex_case_sensitive() {
        // Pattern should not match with different case unless (?i) is in pattern
        let rules = vec![rule(
            UnlockMatch::BodyRegex { pattern: "BLOCKED".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(&outcome(200, "blocked content", "http://x.com/", vec![]), &rules),
            UnlockStatus::Failed
        );
    }

    // ── RedirectMatches ───────────────────────────────────────────────────────

    #[test]
    fn redirect_matches_final_url() {
        let rules = vec![rule(
            UnlockMatch::RedirectMatches { pattern: "geo-block".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(
                &outcome(200, "", "https://example.com/geo-block/error", vec![]),
                &rules
            ),
            UnlockStatus::Blocked
        );
    }

    #[test]
    fn redirect_matches_intermediate_redirect() {
        let rules = vec![rule(
            UnlockMatch::RedirectMatches { pattern: "not-available".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(
                &outcome(
                    200,
                    "",
                    "https://example.com/home",
                    vec!["https://example.com/not-available"]
                ),
                &rules
            ),
            UnlockStatus::Blocked
        );
    }

    #[test]
    fn redirect_matches_no_match() {
        let rules = vec![rule(
            UnlockMatch::RedirectMatches { pattern: "geo-block".to_string() },
            UnlockStatus::Blocked,
        )];
        assert_eq!(
            apply_rules(&outcome(200, "", "https://example.com/home", vec![]), &rules),
            UnlockStatus::Failed
        );
    }

    // ── First-rule-wins + ordering ────────────────────────────────────────────

    #[test]
    fn first_matching_rule_wins() {
        let rules = vec![
            rule(UnlockMatch::StatusEquals { code: 200 }, UnlockStatus::Unlocked),
            rule(UnlockMatch::StatusEquals { code: 200 }, UnlockStatus::Blocked),
        ];
        assert_eq!(apply_rules(&outcome(200, "", "http://x.com/", vec![]), &rules), UnlockStatus::Unlocked);
    }

    #[test]
    fn no_rules_returns_failed() {
        assert_eq!(apply_rules(&outcome(200, "", "http://x.com/", vec![]), &[]), UnlockStatus::Failed);
    }
}
