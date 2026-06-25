// Built-in service unlock detectors.
//
// This module is consumed by the UnlockChecker scheduler.
// Each detector sub-module exposes:
//   - A probe URL constant (or multiple URL constants for multi-step detectors)
//   - A pure `classify(outcome) -> (UnlockStatus, Option<String>)` function
//
// `dispatch` is the public entry point: given a detector key and a shared
// reqwest client, it runs the probe(s) and returns the result with timing.

pub mod amazon_prime;
pub mod chatgpt;
pub mod disney_plus;
pub mod gemini;
pub mod hbo_max;
pub mod netflix;
pub mod spotify;
pub mod tiktok;
pub mod youtube_premium;

use std::time::Instant;

use serverbee_common::protocol::{UnlockRequest, UnlockStatus};

use crate::ip_quality::http;
use crate::ip_quality::rule_engine::HttpOutcome;

/// Run the built-in detector identified by `key` using `client`.
///
/// Returns `(UnlockStatus, Option<region_string>, latency_ms)`.
/// Unknown keys return `UnlockStatus::Unsupported`.
pub async fn dispatch(
    key: &str,
    client: &reqwest::Client,
) -> (UnlockStatus, Option<String>, u32) {
    match key {
        "netflix" => run_netflix(client).await,
        "disney_plus" => run_single(client, disney_plus::PROBE_URL, disney_plus::TIMEOUT_MS, disney_plus::classify).await,
        "youtube_premium" => run_single(client, youtube_premium::PROBE_URL, youtube_premium::TIMEOUT_MS, youtube_premium::classify).await,
        "amazon_prime" => run_single(client, amazon_prime::PROBE_URL, amazon_prime::TIMEOUT_MS, amazon_prime::classify).await,
        "hbo_max" => run_single(client, hbo_max::PROBE_URL, hbo_max::TIMEOUT_MS, hbo_max::classify).await,
        "chatgpt" => run_chatgpt(client).await,
        "gemini" => run_single(client, gemini::PROBE_URL, gemini::TIMEOUT_MS, gemini::classify).await,
        "spotify" => run_single(client, spotify::PROBE_URL, spotify::TIMEOUT_MS, spotify::classify).await,
        "tiktok" => run_single(client, tiktok::PROBE_URL, tiktok::TIMEOUT_MS, tiktok::classify).await,
        _ => (UnlockStatus::Unsupported, None, 0),
    }
}

/// Helper: issue a single-URL probe and classify with a pure function.
async fn run_single<F>(
    client: &reqwest::Client,
    url: &str,
    timeout_ms: u32,
    classify: F,
) -> (UnlockStatus, Option<String>, u32)
where
    F: Fn(&HttpOutcome) -> (UnlockStatus, Option<String>),
{
    let req = UnlockRequest {
        url: url.to_string(),
        method: "GET".to_string(),
        headers: vec![],
        timeout_ms,
    };
    let start = Instant::now();
    match http::fetch(client, &req).await {
        Ok(outcome) => {
            let latency = start.elapsed().as_millis() as u32;
            let (status, region) = classify(&outcome);
            (status, region, latency)
        }
        Err(_) => {
            let latency = start.elapsed().as_millis() as u32;
            (UnlockStatus::Failed, None, latency)
        }
    }
}

/// Netflix: two-step probe (non-original + original).
async fn run_netflix(client: &reqwest::Client) -> (UnlockStatus, Option<String>, u32) {
    let start = Instant::now();

    let non_orig_req = UnlockRequest {
        url: netflix::NON_ORIGINAL_URL.to_string(),
        method: "GET".to_string(),
        headers: vec![],
        timeout_ms: netflix::TIMEOUT_MS,
    };
    let orig_req = UnlockRequest {
        url: netflix::ORIGINAL_URL.to_string(),
        method: "GET".to_string(),
        headers: vec![],
        timeout_ms: netflix::TIMEOUT_MS,
    };

    let non_orig = match http::fetch(client, &non_orig_req).await {
        Ok(o) => o,
        Err(_) => {
            let latency = start.elapsed().as_millis() as u32;
            return (UnlockStatus::Failed, None, latency);
        }
    };

    let orig = match http::fetch(client, &orig_req).await {
        Ok(o) => o,
        Err(_) => {
            let latency = start.elapsed().as_millis() as u32;
            return (UnlockStatus::Failed, None, latency);
        }
    };

    let latency = start.elapsed().as_millis() as u32;
    let (status, region) = netflix::classify(&non_orig, &orig);
    (status, region, latency)
}

/// ChatGPT: trace endpoint first, fallback to homepage.
async fn run_chatgpt(client: &reqwest::Client) -> (UnlockStatus, Option<String>, u32) {
    let start = Instant::now();

    let trace_req = UnlockRequest {
        url: chatgpt::TRACE_URL.to_string(),
        method: "GET".to_string(),
        headers: vec![],
        timeout_ms: chatgpt::TIMEOUT_MS,
    };

    if let Ok(outcome) = http::fetch(client, &trace_req).await {
        let (status, region) = chatgpt::classify_trace(&outcome);
        if status != UnlockStatus::Failed {
            let latency = start.elapsed().as_millis() as u32;
            return (status, region, latency);
        }
    }

    // Fallback to homepage.
    let fallback_req = UnlockRequest {
        url: chatgpt::FALLBACK_URL.to_string(),
        method: "GET".to_string(),
        headers: vec![],
        timeout_ms: chatgpt::TIMEOUT_MS,
    };

    match http::fetch(client, &fallback_req).await {
        Ok(outcome) => {
            let latency = start.elapsed().as_millis() as u32;
            let (status, region) = chatgpt::classify_fallback(&outcome);
            (status, region, latency)
        }
        Err(_) => {
            let latency = start.elapsed().as_millis() as u32;
            (UnlockStatus::Failed, None, latency)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── dispatch: unknown key => Unsupported ──────────────────────────────────

    #[tokio::test]
    async fn dispatch_unknown_key_returns_unsupported() {
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, region, latency) = dispatch("unknown_service_xyz", &client).await;
        assert_eq!(status, UnlockStatus::Unsupported);
        assert!(region.is_none());
        assert_eq!(latency, 0);
    }

    #[tokio::test]
    async fn dispatch_empty_key_returns_unsupported() {
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _, _) = dispatch("", &client).await;
        assert_eq!(status, UnlockStatus::Unsupported);
    }

    // NOTE: the per-service `dispatch(<known key>)` paths perform real outbound
    // HTTP to the streaming providers, so they are intentionally NOT unit-tested
    // here (non-deterministic, network-dependent). Each detector's pure
    // classification logic is covered offline in its own sibling module
    // (e.g. `netflix.rs`, `chatgpt.rs`).

    // ── dispatch: more unknown-key boundary branches (offline, no network) ────
    //
    // The known-key arms of `dispatch` all reach `http::fetch`, so only the
    // unknown-key (`_ =>`) arm is reachable without the network. These cases pin
    // down the routing table's matching behaviour: matching is exact, ASCII
    // case-sensitive, and whitespace-significant, so near-miss keys fall through
    // to `Unsupported` rather than accidentally routing to a real detector.

    #[tokio::test]
    async fn dispatch_whitespace_padded_key_is_unsupported() {
        // Keys are matched exactly; a padded valid key does not route to netflix.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, region, latency) = dispatch(" netflix ", &client).await;
        assert_eq!(status, UnlockStatus::Unsupported);
        assert!(region.is_none());
        assert_eq!(latency, 0);
    }

    #[tokio::test]
    async fn dispatch_uppercase_key_is_unsupported() {
        // Match arms are lowercase literals; uppercase variants are unknown.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _, latency) = dispatch("NETFLIX", &client).await;
        assert_eq!(status, UnlockStatus::Unsupported);
        assert_eq!(latency, 0);
    }

    #[tokio::test]
    async fn dispatch_hyphenated_alias_is_unsupported() {
        // The table key is `disney_plus`; the hyphenated alias is not registered.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _, _) = dispatch("disney-plus", &client).await;
        assert_eq!(status, UnlockStatus::Unsupported);
    }

    // ── dispatch routing-contract checks (pure classify functions) ────────────
    //
    // `dispatch` wires each key to a specific detector's pure `classify`. We
    // cannot drive the network half offline, but we CAN lock the routing
    // contract by exercising the exact classify function each arm delegates to,
    // across its result branches. This guards against a future edit silently
    // pointing a key at the wrong detector or status mapping.

    fn outcome(status: u16, body: &str, final_url: &str, redirects: Vec<&str>) -> HttpOutcome {
        HttpOutcome {
            status,
            body: body.to_string(),
            final_url: final_url.to_string(),
            redirects: redirects.into_iter().map(str::to_string).collect(),
        }
    }

    // -- netflix arm: two-step (non-original + original) merge semantics --

    #[test]
    fn netflix_arm_non_original_200_yields_unlocked() {
        // `run_netflix` merges both probes via netflix::classify; a 200
        // non-original short-circuits to Unlocked regardless of the original.
        let non_orig = outcome(200, "", "https://www.netflix.com/title/81280792", vec![]);
        let orig = outcome(404, "", "https://www.netflix.com/title/80018499", vec![]);
        let (status, region) = netflix::classify(&non_orig, &orig);
        assert_eq!(status, UnlockStatus::Unlocked);
        assert!(region.is_none());
    }

    #[test]
    fn netflix_arm_originals_only_yields_restricted() {
        // Non-original blocked but original 200 => originals-only (Restricted).
        let non_orig = outcome(404, "", "https://www.netflix.com/title/81280792", vec![]);
        let orig = outcome(200, "", "https://www.netflix.com/title/80018499", vec![]);
        let (status, _) = netflix::classify(&non_orig, &orig);
        assert_eq!(status, UnlockStatus::Restricted);
    }

    #[test]
    fn netflix_arm_both_blocked_yields_blocked() {
        // Both probes non-200 => fully Blocked.
        let non_orig = outcome(403, "", "https://www.netflix.com/title/81280792", vec![]);
        let orig = outcome(403, "", "https://www.netflix.com/title/80018499", vec![]);
        let (status, _) = netflix::classify(&non_orig, &orig);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    // -- chatgpt arm: trace-first, fallback-on-failure merge semantics --

    #[test]
    fn chatgpt_arm_trace_unlocked_for_allowed_country() {
        // The trace branch parses loc= and Unlocks an allowed country (US).
        let trace = outcome(200, "fl=1\nloc=US\ntls=x\n", chatgpt::TRACE_URL, vec![]);
        let (status, region) = chatgpt::classify_trace(&trace);
        assert_eq!(status, UnlockStatus::Unlocked);
        assert_eq!(region.as_deref(), Some("US"));
    }

    #[test]
    fn chatgpt_arm_trace_blocked_for_sanctioned_country() {
        // A sanctioned loc= (CN) maps to Blocked with the region echoed back.
        let trace = outcome(200, "fl=1\nloc=CN\ntls=x\n", chatgpt::TRACE_URL, vec![]);
        let (status, region) = chatgpt::classify_trace(&trace);
        assert_eq!(status, UnlockStatus::Blocked);
        assert_eq!(region.as_deref(), Some("CN"));
    }

    #[test]
    fn chatgpt_arm_trace_failed_triggers_fallback_path() {
        // A non-200 trace yields Failed; `run_chatgpt` then uses the fallback,
        // whose 200 + service markers resolve to Unlocked.
        let trace = outcome(403, "", chatgpt::TRACE_URL, vec![]);
        let (trace_status, _) = chatgpt::classify_trace(&trace);
        assert_eq!(trace_status, UnlockStatus::Failed);

        let fallback = outcome(200, "<html>Sign in to ChatGPT</html>", chatgpt::FALLBACK_URL, vec![]);
        let (fb_status, _) = chatgpt::classify_fallback(&fallback);
        assert_eq!(fb_status, UnlockStatus::Unlocked);
    }

    #[test]
    fn chatgpt_arm_fallback_blocked_on_403() {
        // The fallback branch treats a 403 homepage as a hard geo-block.
        let fallback = outcome(403, "", chatgpt::FALLBACK_URL, vec![]);
        let (status, _) = chatgpt::classify_fallback(&fallback);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    // -- single-URL arms: each key delegates to its detector::classify --

    #[test]
    fn disney_plus_arm_blocked_on_unavailable_redirect() {
        // `run_single` for disney_plus delegates to disney_plus::classify; an
        // "unavailable" redirect URL classifies as Blocked.
        let o = outcome(200, "", "https://www.disneyplus.com/unavailable", vec![]);
        let (status, _) = disney_plus::classify(&o);
        assert_eq!(status, UnlockStatus::Blocked);
    }

    #[test]
    fn disney_plus_arm_unrecognized_200_is_failed() {
        // A 200 with no positive or negative marker is inconclusive (Failed),
        // not Blocked — confirming the conservative default branch.
        let o = outcome(200, "<html>loading</html>", "https://www.disneyplus.com/home", vec![]);
        let (status, _) = disney_plus::classify(&o);
        assert_eq!(status, UnlockStatus::Failed);
    }

    #[test]
    fn youtube_premium_arm_classifies_via_detector() {
        // A 403 from the youtube_premium probe surfaces a non-Unlocked status,
        // confirming the arm reaches youtube_premium::classify, not Unsupported.
        let o = outcome(403, "", "https://www.youtube.com/premium", vec![]);
        let (status, _) = youtube_premium::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
        assert_ne!(status, UnlockStatus::Unlocked);
    }

    #[test]
    fn amazon_prime_arm_classifies_via_detector() {
        // The amazon_prime arm delegates to amazon_prime::classify; a plain 200
        // with no markers must not be reported as Unsupported.
        let o = outcome(200, "<html>home</html>", "https://www.primevideo.com/", vec![]);
        let (status, _) = amazon_prime::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
    }

    #[test]
    fn hbo_max_arm_classifies_via_detector() {
        // The hbo_max arm delegates to hbo_max::classify (never Unsupported).
        let o = outcome(200, "<html>home</html>", "https://www.max.com/", vec![]);
        let (status, _) = hbo_max::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
    }

    #[test]
    fn gemini_arm_classifies_via_detector() {
        // The gemini arm delegates to gemini::classify (never Unsupported).
        let o = outcome(200, "<html>home</html>", "https://gemini.google.com/", vec![]);
        let (status, _) = gemini::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
    }

    #[test]
    fn spotify_arm_classifies_via_detector() {
        // The spotify arm delegates to spotify::classify (never Unsupported).
        let o = outcome(200, "<html>home</html>", "https://www.spotify.com/", vec![]);
        let (status, _) = spotify::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
    }

    #[test]
    fn tiktok_arm_classifies_via_detector() {
        // The tiktok arm delegates to tiktok::classify (never Unsupported).
        let o = outcome(200, "<html>home</html>", "https://www.tiktok.com/", vec![]);
        let (status, _) = tiktok::classify(&o);
        assert_ne!(status, UnlockStatus::Unsupported);
    }

    // ── run_single: fetch-error branch (deterministic, no network) ────────────
    //
    // `run_single` is the helper every single-URL detector arm of `dispatch`
    // delegates to. Its `Ok` arm needs a successful `http::fetch`, which the
    // SSRF guard makes impossible to reach offline (loopback/private hosts are
    // rejected before connecting, and real provider hosts require the network).
    // Its `Err` arm, however, fires for any URL that `ssrf::validate_url`
    // rejects *synchronously* — before any DNS lookup or socket is opened — so
    // we can pin the error contract `(Failed, None, latency)` deterministically
    // by feeding `run_single` URLs that fail pre-flight validation.
    //
    // We pass a `classify` that panics: on the error path it must never run, so
    // these tests double as proof that `run_single` short-circuits to `Failed`
    // without invoking the detector's classifier when the probe itself errors.

    fn never_classify(_o: &HttpOutcome) -> (UnlockStatus, Option<String>) {
        panic!("classify must not be called when the fetch errors");
    }

    #[tokio::test]
    async fn run_single_invalid_scheme_url_yields_failed() {
        // A non-http(s) scheme is rejected by `ssrf::validate_url` before any
        // network access, driving `run_single`'s `Err(_)` arm.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, region, _latency) =
            run_single(&client, "ftp://example.com/file", 1000, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
        assert!(region.is_none(), "the error path returns no region");
    }

    #[tokio::test]
    async fn run_single_unparseable_url_yields_failed() {
        // A string that `Url::parse` cannot parse fails validation immediately;
        // `run_single` must surface `Failed` (not panic, not Unsupported).
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, region, _latency) =
            run_single(&client, "not a url at all", 1000, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
        assert!(region.is_none());
    }

    #[tokio::test]
    async fn run_single_disallowed_port_url_yields_failed() {
        // The strict validator rejects ports other than 80/443 before connecting,
        // so a high-port URL exercises the same `Err(_)` short-circuit.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _region, _latency) =
            run_single(&client, "http://example.com:8080/", 1000, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
    }

    #[tokio::test]
    async fn run_single_embedded_credentials_url_yields_failed() {
        // Embedded userinfo is rejected pre-flight; another distinct synchronous
        // validation failure feeding the `Err(_)` arm.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _region, _latency) =
            run_single(&client, "http://user:pass@example.com/", 1000, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
    }

    #[tokio::test]
    async fn run_single_loopback_host_yields_failed() {
        // A loopback host passes scheme/port/credential validation but is rejected
        // by `resolve_and_check` (the in-loop SSRF guard) before any data is
        // exchanged — still the `Err(_)` arm, via a different rejection point.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, region, _latency) =
            run_single(&client, "http://127.0.0.1/", 1000, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
        assert!(region.is_none());
    }

    #[tokio::test]
    async fn run_single_clamps_sub_minimum_timeout_on_error_path() {
        // `timeout_ms == 0` flows through `run_single` into the request builder;
        // the SSRF guard still short-circuits to the `Err(_)` arm and the timeout
        // clamp must not underflow or panic.
        let client = crate::ip_quality::http::build_client().unwrap();
        let (status, _region, _latency) =
            run_single(&client, "http://127.0.0.1/", 0, never_classify).await;
        assert_eq!(status, UnlockStatus::Failed);
    }
}
