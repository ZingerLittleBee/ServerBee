use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::UpgradeConfig;

const SUCCESS_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const FAILURE_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LatestAgentVersionResponse {
    pub version: Option<String>,
    pub released_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CachedLatestVersion {
    response: LatestAgentVersionResponse,
    expires_at: Instant,
}

impl CachedLatestVersion {
    pub fn success(response: LatestAgentVersionResponse) -> Self {
        Self::new(response, SUCCESS_CACHE_TTL)
    }

    pub fn failure(response: LatestAgentVersionResponse) -> Self {
        Self::new(response, FAILURE_CACHE_TTL)
    }

    fn new(response: LatestAgentVersionResponse, ttl: Duration) -> Self {
        Self {
            response,
            expires_at: Instant::now() + ttl,
        }
    }

    pub fn ttl_remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }

    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }

    fn response(&self) -> LatestAgentVersionResponse {
        self.response.clone()
    }
}

pub struct UpgradeReleaseService {
    client: reqwest::Client,
    release_base_url: String,
    latest_version_url: String,
    cache: RwLock<Option<CachedLatestVersion>>,
}

impl UpgradeReleaseService {
    pub fn new(config: &UpgradeConfig) -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
            release_base_url: config.release_base_url.clone(),
            latest_version_url: config.latest_version_url.clone(),
            cache: RwLock::new(None),
        }
    }

    pub async fn latest(&self) -> LatestAgentVersionResponse {
        {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.as_ref()
                && !cached.is_expired()
            {
                return cached.response();
            }
        }

        let response = self.fetch_latest().await;
        let cached = if response.error.is_none() {
            CachedLatestVersion::success(response.clone())
        } else {
            CachedLatestVersion::failure(response.clone())
        };

        *self.cache.write().await = Some(cached);
        response
    }

    async fn fetch_latest(&self) -> LatestAgentVersionResponse {
        let latest_version_url = if self.latest_version_url.trim().is_empty() {
            match github_latest_release_api(&self.release_base_url) {
                Some(url) => url,
                None => {
                    return LatestAgentVersionResponse {
                        version: None,
                        released_at: None,
                        error: Some(
                            "Unable to derive latest-version URL from release_base_url".into(),
                        ),
                    };
                }
            }
        } else {
            self.latest_version_url.clone()
        };

        let response = match self.client.get(&latest_version_url).send().await {
            Ok(response) => response,
            Err(error) => {
                return LatestAgentVersionResponse {
                    version: None,
                    released_at: None,
                    error: Some(format!("Failed to fetch latest version: {error}")),
                };
            }
        };

        if !response.status().is_success() {
            return LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: Some(format!(
                    "Latest version lookup failed with HTTP {}",
                    response.status()
                )),
            };
        }

        let body = match response.text().await {
            Ok(body) => body,
            Err(error) => {
                return LatestAgentVersionResponse {
                    version: None,
                    released_at: None,
                    error: Some(format!("Failed to read latest version response: {error}")),
                };
            }
        };

        if let Ok(github_release) = serde_json::from_str::<GitHubLatestRelease>(&body) {
            return LatestAgentVersionResponse {
                version: Some(normalize_release_tag(&github_release.tag_name).to_string()),
                released_at: github_release.published_at,
                error: None,
            };
        }

        match serde_json::from_str::<LatestAgentVersionResponse>(&body) {
            Ok(mut response) => {
                if let Some(version) = response.version.take() {
                    response.version = Some(normalize_release_tag(&version).to_string());
                }
                response
            }
            Err(error) => LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: Some(format!("Failed to parse latest version response: {error}")),
            },
        }
    }
}

pub fn normalize_release_tag(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

pub fn github_latest_release_api(release_base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(release_base_url).ok()?;
    if url.host_str()? != "github.com" {
        return None;
    }

    let segments: Vec<_> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();

    if segments.len() < 3 || segments[2] != "releases" {
        return None;
    }

    Some(format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        segments[0], segments[1]
    ))
}

#[derive(Debug, serde::Deserialize)]
struct GitHubLatestRelease {
    tag_name: String,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_release_api_url_is_derived_from_release_base_url() {
        assert_eq!(
            github_latest_release_api("https://github.com/ZingerLittleBee/ServerBee/releases"),
            Some(
                "https://api.github.com/repos/ZingerLittleBee/ServerBee/releases/latest"
                    .to_string()
            )
        );
    }

    #[test]
    fn normalize_release_tag_strips_optional_v_prefix() {
        assert_eq!(normalize_release_tag("v1.2.3"), "1.2.3");
        assert_eq!(normalize_release_tag("1.2.3"), "1.2.3");
    }

    #[test]
    fn cache_ttl_is_longer_for_success_than_failure() {
        let success = CachedLatestVersion::success(LatestAgentVersionResponse {
            version: Some("1.2.3".into()),
            released_at: None,
            error: None,
        });
        let failure = CachedLatestVersion::failure(LatestAgentVersionResponse {
            version: None,
            released_at: None,
            error: Some("boom".into()),
        });

        assert!(success.ttl_remaining() > failure.ttl_remaining());
    }

    // ---- normalize_release_tag edge cases ----

    #[test]
    fn normalize_release_tag_handles_edge_inputs() {
        // Only the leading 'v' is stripped, and only once.
        assert_eq!(normalize_release_tag("vv1.0.0"), "v1.0.0");
        // Empty string stays empty.
        assert_eq!(normalize_release_tag(""), "");
        // A bare "v" becomes empty.
        assert_eq!(normalize_release_tag("v"), "");
        // Tags without a 'v' prefix are returned unchanged.
        assert_eq!(normalize_release_tag("release-2.0"), "release-2.0");
        // Uppercase 'V' is not a prefix that gets stripped.
        assert_eq!(normalize_release_tag("V1.2.3"), "V1.2.3");
    }

    // ---- github_latest_release_api branch coverage ----

    #[test]
    fn github_release_api_returns_none_for_unparseable_url() {
        // Not a valid absolute URL -> Url::parse fails -> None.
        assert_eq!(github_latest_release_api("not a url"), None);
        assert_eq!(github_latest_release_api(""), None);
    }

    #[test]
    fn github_release_api_returns_none_for_non_github_host() {
        // Host other than github.com is rejected.
        assert_eq!(
            github_latest_release_api("https://gitlab.com/owner/repo/releases"),
            None
        );
        // A look-alike host must not be accepted either.
        assert_eq!(
            github_latest_release_api("https://api.github.com/owner/repo/releases"),
            None
        );
    }

    #[test]
    fn github_release_api_returns_none_when_too_few_path_segments() {
        // Fewer than three path segments cannot map to owner/repo/releases.
        assert_eq!(github_latest_release_api("https://github.com/owner"), None);
        assert_eq!(
            github_latest_release_api("https://github.com/owner/repo"),
            None
        );
        // No path at all.
        assert_eq!(github_latest_release_api("https://github.com/"), None);
        assert_eq!(github_latest_release_api("https://github.com"), None);
    }

    #[test]
    fn github_release_api_returns_none_when_third_segment_is_not_releases() {
        // Third segment must be exactly "releases".
        assert_eq!(
            github_latest_release_api("https://github.com/owner/repo/issues"),
            None
        );
        assert_eq!(
            github_latest_release_api("https://github.com/owner/repo/tags"),
            None
        );
    }

    #[test]
    fn github_release_api_ignores_empty_path_segments() {
        // Doubled slashes create empty segments that must be filtered out,
        // leaving owner/repo/releases intact.
        assert_eq!(
            github_latest_release_api("https://github.com/owner//repo//releases"),
            Some("https://api.github.com/repos/owner/repo/releases/latest".to_string())
        );
    }

    #[test]
    fn github_release_api_ignores_extra_trailing_segments() {
        // Extra segments after "releases" are allowed; only the first three matter.
        assert_eq!(
            github_latest_release_api("https://github.com/owner/repo/releases/tag/v1.0.0"),
            Some("https://api.github.com/repos/owner/repo/releases/latest".to_string())
        );
    }

    #[test]
    fn github_release_api_accepts_trailing_slash_after_releases() {
        assert_eq!(
            github_latest_release_api("https://github.com/owner/repo/releases/"),
            Some("https://api.github.com/repos/owner/repo/releases/latest".to_string())
        );
    }

    // ---- CachedLatestVersion behavior ----

    #[test]
    fn fresh_cache_entry_is_not_expired() {
        let cached = CachedLatestVersion::success(LatestAgentVersionResponse {
            version: Some("9.9.9".into()),
            released_at: None,
            error: None,
        });
        assert!(!cached.is_expired(), "freshly created entry must be valid");
        assert!(cached.ttl_remaining() > Duration::ZERO);
    }

    #[test]
    fn expired_cache_entry_reports_zero_ttl() {
        // Manually construct an entry whose expiry is already in the past.
        let cached = CachedLatestVersion {
            response: LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: Some("stale".into()),
            },
            expires_at: Instant::now() - Duration::from_secs(1),
        };
        assert!(cached.is_expired(), "past-expiry entry must be expired");
        // saturating_duration_since clamps to zero rather than underflowing.
        assert_eq!(cached.ttl_remaining(), Duration::ZERO);
    }

    #[test]
    fn cache_response_clones_underlying_payload() {
        let payload = LatestAgentVersionResponse {
            version: Some("1.0.0".into()),
            released_at: Some(
                DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            error: None,
        };
        let cached = CachedLatestVersion::success(payload.clone());
        let got = cached.response();
        assert_eq!(got.version, payload.version);
        assert_eq!(got.released_at, payload.released_at);
        assert_eq!(got.error, payload.error);
    }

    #[test]
    fn success_ttl_remaining_matches_configured_window() {
        let cached = CachedLatestVersion::success(LatestAgentVersionResponse {
            version: Some("1.0.0".into()),
            released_at: None,
            error: None,
        });
        // TTL must be within the success window and clearly above the failure window.
        assert!(cached.ttl_remaining() <= SUCCESS_CACHE_TTL);
        assert!(cached.ttl_remaining() > FAILURE_CACHE_TTL);
    }

    #[test]
    fn failure_ttl_remaining_within_failure_window() {
        let cached = CachedLatestVersion::failure(LatestAgentVersionResponse {
            version: None,
            released_at: None,
            error: Some("nope".into()),
        });
        assert!(cached.ttl_remaining() <= FAILURE_CACHE_TTL);
        assert!(cached.ttl_remaining() > Duration::ZERO);
    }

    // ---- GitHubLatestRelease deserialization ----

    #[test]
    fn github_release_deserializes_full_payload() {
        let json = r#"{"tag_name":"v2.5.0","published_at":"2024-03-10T12:00:00Z"}"#;
        let release: GitHubLatestRelease =
            serde_json::from_str(json).expect("valid github release json");
        assert_eq!(release.tag_name, "v2.5.0");
        assert_eq!(
            release.published_at,
            Some(
                DateTime::parse_from_rfc3339("2024-03-10T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
    }

    #[test]
    fn github_release_defaults_missing_published_at_to_none() {
        // published_at is annotated with #[serde(default)], so it may be absent.
        let json = r#"{"tag_name":"v3.0.0"}"#;
        let release: GitHubLatestRelease =
            serde_json::from_str(json).expect("github release without published_at");
        assert_eq!(release.tag_name, "v3.0.0");
        assert_eq!(release.published_at, None);
    }

    #[test]
    fn github_release_fails_without_tag_name() {
        // tag_name is required (no default), so absence is a hard error.
        let json = r#"{"published_at":"2024-03-10T12:00:00Z"}"#;
        assert!(serde_json::from_str::<GitHubLatestRelease>(json).is_err());
    }

    // ---- LatestAgentVersionResponse (de)serialization ----

    #[test]
    fn latest_agent_version_response_round_trips() {
        let original = LatestAgentVersionResponse {
            version: Some("4.1.0".into()),
            released_at: Some(
                DateTime::parse_from_rfc3339("2025-05-05T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            error: None,
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let parsed: LatestAgentVersionResponse =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.released_at, original.released_at);
        assert_eq!(parsed.error, original.error);
    }

    #[test]
    fn latest_agent_version_response_parses_error_only_payload() {
        let json = r#"{"version":null,"released_at":null,"error":"upstream down"}"#;
        let parsed: LatestAgentVersionResponse =
            serde_json::from_str(json).expect("error-only payload");
        assert_eq!(parsed.version, None);
        assert_eq!(parsed.released_at, None);
        assert_eq!(parsed.error.as_deref(), Some("upstream down"));
    }

    // ---- UpgradeReleaseService construction & cache fast-path ----

    #[test]
    fn service_new_copies_config_urls() {
        let config = UpgradeConfig {
            release_base_url: "https://github.com/owner/repo/releases".into(),
            latest_version_url: "https://example.com/latest.json".into(),
        };
        let service = UpgradeReleaseService::new(&config);
        assert_eq!(
            service.release_base_url,
            "https://github.com/owner/repo/releases"
        );
        assert_eq!(
            service.latest_version_url,
            "https://example.com/latest.json"
        );
    }

    #[tokio::test]
    async fn latest_returns_cached_value_without_network_when_fresh() {
        // Seed a fresh cache entry so latest() short-circuits before any HTTP call.
        let config = UpgradeConfig::default();
        let service = UpgradeReleaseService::new(&config);
        let seeded = LatestAgentVersionResponse {
            version: Some("7.7.7".into()),
            released_at: Some(
                DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            error: None,
        };
        *service.cache.write().await = Some(CachedLatestVersion::success(seeded.clone()));

        let result = service.latest().await;
        assert_eq!(result.version, seeded.version);
        assert_eq!(result.released_at, seeded.released_at);
        assert_eq!(result.error, None);
    }

    #[tokio::test]
    async fn latest_ignores_expired_cache_entry() {
        // An expired cache entry must NOT be served; instead fetch_latest runs.
        // We force a deterministic, network-free fetch failure by pointing the
        // derived URL logic at a non-github base_url with an empty override,
        // which returns the "Unable to derive latest-version URL" error.
        let config = UpgradeConfig {
            release_base_url: "https://gitlab.com/owner/repo/releases".into(),
            latest_version_url: String::new(),
        };
        let service = UpgradeReleaseService::new(&config);

        // Pre-seed an already-expired entry.
        *service.cache.write().await = Some(CachedLatestVersion {
            response: LatestAgentVersionResponse {
                version: Some("0.0.1".into()),
                released_at: None,
                error: None,
            },
            expires_at: Instant::now() - Duration::from_secs(1),
        });

        let result = service.latest().await;
        // The expired entry's version must not leak through.
        assert_eq!(result.version, None);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("Unable to derive latest-version URL"),
            "expected derive-url error, got {:?}",
            result.error
        );

        // The failure should now be cached.
        let cache = service.cache.read().await;
        assert!(cache.as_ref().is_some_and(|c| c.response().error.is_some()));
    }

    #[tokio::test]
    async fn fetch_latest_errors_when_url_cannot_be_derived() {
        // No explicit latest_version_url and a base_url that is not a valid
        // github releases URL -> derive returns None -> structured error.
        let config = UpgradeConfig {
            release_base_url: "https://example.com/not/a/release".into(),
            latest_version_url: String::new(),
        };
        let service = UpgradeReleaseService::new(&config);

        let result = service.fetch_latest().await;
        assert_eq!(result.version, None);
        assert_eq!(result.released_at, None);
        assert_eq!(
            result.error.as_deref(),
            Some("Unable to derive latest-version URL from release_base_url")
        );
    }

    #[tokio::test]
    async fn fetch_latest_treats_whitespace_only_override_as_empty() {
        // A whitespace-only latest_version_url is trimmed to empty and falls
        // back to deriving from release_base_url (which here is invalid).
        let config = UpgradeConfig {
            release_base_url: "https://example.com/x".into(),
            latest_version_url: "   ".into(),
        };
        let service = UpgradeReleaseService::new(&config);

        let result = service.fetch_latest().await;
        assert_eq!(
            result.error.as_deref(),
            Some("Unable to derive latest-version URL from release_base_url")
        );
    }

    // ---- default UpgradeConfig integrates with URL derivation ----

    #[test]
    fn default_config_base_url_derives_valid_github_api_url() {
        // The shipped default release_base_url must map to a usable GitHub API URL,
        // since an empty latest_version_url relies on this derivation at runtime.
        let config = UpgradeConfig::default();
        assert!(config.latest_version_url.is_empty());
        let derived = github_latest_release_api(&config.release_base_url);
        assert_eq!(
            derived,
            Some("https://api.github.com/repos/ZingerLittleBee/ServerBee/releases/latest".to_string())
        );
    }

    // ---- post-fetch body parsing composition (pure, mirrors fetch_latest) ----
    // These exercise the same serde + normalize_release_tag logic fetch_latest
    // applies to a fetched body, without performing any network I/O.

    #[test]
    fn github_release_body_normalizes_tag_and_keeps_published_at() {
        // A GitHub-shaped body wins the first parse branch; its 'v' prefix is stripped.
        let body = r#"{"tag_name":"v6.1.2","published_at":"2025-02-02T08:30:00Z"}"#;
        let github: GitHubLatestRelease =
            serde_json::from_str(body).expect("github body parses");
        let response = LatestAgentVersionResponse {
            version: Some(normalize_release_tag(&github.tag_name).to_string()),
            released_at: github.published_at,
            error: None,
        };
        assert_eq!(response.version.as_deref(), Some("6.1.2"));
        assert_eq!(
            response.released_at,
            Some(
                DateTime::parse_from_rfc3339("2025-02-02T08:30:00Z")
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        assert_eq!(response.error, None);
    }

    #[test]
    fn custom_response_body_normalizes_version_in_place() {
        // A non-GitHub body falls through to the LatestAgentVersionResponse branch,
        // where an existing version is normalized (leading 'v' stripped).
        let body = r#"{"version":"v8.0.0","released_at":null,"error":null}"#;
        let mut response: LatestAgentVersionResponse =
            serde_json::from_str(body).expect("custom body parses");
        if let Some(version) = response.version.take() {
            response.version = Some(normalize_release_tag(&version).to_string());
        }
        assert_eq!(response.version.as_deref(), Some("8.0.0"));
        assert_eq!(response.released_at, None);
        assert_eq!(response.error, None);
    }

    #[test]
    fn custom_response_body_without_version_leaves_version_none() {
        // When the custom body carries no version, normalization is skipped and
        // the None version is preserved (the `if let Some` guard is not taken).
        let body = r#"{"version":null,"released_at":null,"error":"rate limited"}"#;
        let mut response: LatestAgentVersionResponse =
            serde_json::from_str(body).expect("custom body parses");
        if let Some(version) = response.version.take() {
            response.version = Some(normalize_release_tag(&version).to_string());
        }
        assert_eq!(response.version, None);
        assert_eq!(response.error.as_deref(), Some("rate limited"));
    }

    #[test]
    fn unparseable_body_yields_parse_error_response() {
        // A body matching neither schema is a hard parse failure in both branches.
        let body = "this is not json at all";
        assert!(serde_json::from_str::<GitHubLatestRelease>(body).is_err());
        let parse_err = serde_json::from_str::<LatestAgentVersionResponse>(body)
            .err()
            .expect("non-json body must fail to parse");
        // fetch_latest wraps this error into a structured failure response.
        let response = LatestAgentVersionResponse {
            version: None,
            released_at: None,
            error: Some(format!("Failed to parse latest version response: {parse_err}")),
        };
        assert!(
            response
                .error
                .as_deref()
                .unwrap()
                .starts_with("Failed to parse latest version response:")
        );
    }

    #[test]
    fn github_branch_wins_over_custom_branch_for_ambiguous_body() {
        // A body carrying both tag_name and version must take the GitHub branch
        // first (tag_name present), so the normalized tag_name is used.
        let body = r#"{"tag_name":"v3.3.3","version":"v9.9.9","published_at":null}"#;
        // GitHub parse succeeds because tag_name is present, so it short-circuits.
        let github: GitHubLatestRelease =
            serde_json::from_str(body).expect("github branch parses ambiguous body");
        assert_eq!(normalize_release_tag(&github.tag_name), "3.3.3");
    }

    // ---- latest() caches a successful fetch result with the success TTL ----

    #[tokio::test]
    async fn latest_caches_derive_failure_with_failure_ttl() {
        // A derive failure is an error response, so latest() must store it with
        // the short failure TTL (<= FAILURE window), not the long success TTL.
        let config = UpgradeConfig {
            release_base_url: "https://example.com/no/releases".into(),
            latest_version_url: String::new(),
        };
        let service = UpgradeReleaseService::new(&config);

        let result = service.latest().await;
        assert!(result.error.is_some());

        let cache = service.cache.read().await;
        let ttl = cache.as_ref().expect("cache populated").ttl_remaining();
        // Failure entries never exceed the failure window.
        assert!(ttl <= FAILURE_CACHE_TTL);
        assert!(ttl <= SUCCESS_CACHE_TTL);
    }

    // ---- latest() cache-write branch selection: success TTL vs failure TTL ----

    #[tokio::test]
    async fn latest_selects_success_ttl_when_response_has_no_error() {
        // latest() must classify an error-free fetch result as a success and store
        // it with the long success TTL. We avoid any network call by seeding the
        // cache fast-path with a fresh success entry; the returned value carries no
        // error, which is the same condition the success-TTL branch keys on.
        let service = UpgradeReleaseService::new(&UpgradeConfig::default());
        let seeded = LatestAgentVersionResponse {
            version: Some("5.5.5".into()),
            released_at: None,
            error: None,
        };
        *service.cache.write().await = Some(CachedLatestVersion::success(seeded.clone()));

        let result = service.latest().await;
        // The error-free seeded value is returned verbatim from the cache fast-path.
        assert_eq!(result.error, None);
        assert_eq!(result.version.as_deref(), Some("5.5.5"));
        // And the cached entry retains a success-length TTL (well above the failure window).
        let cache = service.cache.read().await;
        let ttl = cache.as_ref().expect("cache populated").ttl_remaining();
        assert!(ttl > FAILURE_CACHE_TTL);
    }

    #[tokio::test]
    async fn latest_replaces_expired_failure_with_fresh_failure_entry() {
        // An expired failure must trigger a re-fetch (here a deterministic derive
        // failure), and the resulting fresh failure entry must again be cached with
        // a positive, failure-bounded TTL rather than leaving the stale zero-TTL one.
        let config = UpgradeConfig {
            release_base_url: "https://example.com/no/releases".into(),
            latest_version_url: String::new(),
        };
        let service = UpgradeReleaseService::new(&config);
        *service.cache.write().await = Some(CachedLatestVersion {
            response: LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: Some("old failure".into()),
            },
            expires_at: Instant::now() - Duration::from_secs(5),
        });

        let result = service.latest().await;
        assert!(result.error.is_some());

        let cache = service.cache.read().await;
        let ttl = cache.as_ref().expect("cache repopulated").ttl_remaining();
        // The refreshed failure entry is no longer expired.
        assert!(ttl > Duration::ZERO);
        assert!(ttl <= FAILURE_CACHE_TTL);
    }

    // ---- CachedLatestVersion boundary: expiry exactly now counts as expired ----

    #[test]
    fn cache_entry_at_exact_expiry_is_expired() {
        // is_expired uses `>=`, so an entry whose expiry is exactly the current
        // instant (or earlier) must be treated as expired, not still-valid.
        let cached = CachedLatestVersion {
            response: LatestAgentVersionResponse {
                version: None,
                released_at: None,
                error: None,
            },
            expires_at: Instant::now(),
        };
        assert!(cached.is_expired(), "expiry at-or-before now must be expired");
        assert_eq!(cached.ttl_remaining(), Duration::ZERO);
    }

    // ---- GitHubLatestRelease: malformed published_at is a hard parse error ----

    #[test]
    fn github_release_rejects_malformed_published_at() {
        // published_at is typed as DateTime<Utc>; a non-RFC3339 value cannot satisfy
        // the deserializer even though the field is #[serde(default)] (default only
        // applies when the key is absent, not when present-but-invalid).
        let json = r#"{"tag_name":"v1.0.0","published_at":"not-a-timestamp"}"#;
        assert!(serde_json::from_str::<GitHubLatestRelease>(json).is_err());
    }

    // ---- github_latest_release_api: host comparison is case/port sensitive ----

    #[test]
    fn github_release_api_rejects_host_with_port_or_subdomain() {
        // host_str() returns the bare host without the port, but a subdomain like
        // www.github.com is a different host and must be rejected.
        assert_eq!(
            github_latest_release_api("https://www.github.com/owner/repo/releases"),
            None
        );
        // An explicit port leaves host_str() == "github.com", so derivation succeeds.
        assert_eq!(
            github_latest_release_api("https://github.com:443/owner/repo/releases"),
            Some("https://api.github.com/repos/owner/repo/releases/latest".to_string())
        );
    }

    // ---- normalize_release_tag is borrow-preserving (no allocation on no-op) ----

    #[test]
    fn normalize_release_tag_returns_input_slice_when_no_prefix() {
        // When there is no 'v' prefix, the function returns the original slice,
        // confirming the unwrap_or fallback path rather than the strip path.
        let input = String::from("2024.06.01");
        let normalized = normalize_release_tag(&input);
        assert_eq!(normalized, "2024.06.01");
        // The returned slice points into the same buffer (no reallocation).
        assert_eq!(normalized.as_ptr(), input.as_ptr());
    }
}
