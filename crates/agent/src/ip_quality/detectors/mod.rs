// Built-in service unlock detectors.
//
// This module is consumed by the UnlockChecker scheduler (Unit J).
// Each detector sub-module exposes:
//   - A probe URL constant (or multiple URL constants for multi-step detectors)
//   - A pure `classify(outcome) -> (UnlockStatus, Option<String>)` function
//
// `dispatch` is the public entry point: given a detector key and a shared
// reqwest client, it runs the probe(s) and returns the result with timing.
#![allow(dead_code)]

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

/// Default timeout used for all built-in probes (ms).
const DEFAULT_TIMEOUT_MS: u32 = 15_000;

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
}
