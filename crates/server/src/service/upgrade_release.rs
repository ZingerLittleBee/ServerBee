use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::UpgradeConfig;
use crate::error::AppError;

const SUCCESS_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const FAILURE_CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LatestAgentVersionResponse {
    pub version: Option<String>,
    pub released_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub download_url: String,
    pub sha256: String,
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

    pub async fn resolve_asset(
        &self,
        version: &str,
        asset_name: &str,
    ) -> Result<ReleaseAsset, AppError> {
        let version = normalize_release_tag(version);
        let download_url = format!(
            "{}/download/v{version}/{asset_name}",
            self.release_base_url.trim_end_matches('/')
        );
        let checksums_url = format!(
            "{}/download/v{version}/checksums.txt",
            self.release_base_url.trim_end_matches('/')
        );

        let checksums_response = self
            .client
            .get(&checksums_url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to fetch checksums: {e}")))?;

        if !checksums_response.status().is_success() {
            return Err(AppError::NotFound(format!(
                "Checksums not found for version v{version} (HTTP {})",
                checksums_response.status()
            )));
        }

        let checksums_body = checksums_response
            .text()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read checksums: {e}")))?;

        let sha256 = checksums_body
            .lines()
            .find_map(|line| {
                let mut parts = line.split_whitespace();
                let hash = parts.next()?;
                let name = parts.next()?;
                if name == asset_name {
                    Some(hash.to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "Checksum not found for {asset_name} in v{version} release"
                ))
            })?;

        Ok(ReleaseAsset {
            download_url,
            sha256,
        })
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
}
