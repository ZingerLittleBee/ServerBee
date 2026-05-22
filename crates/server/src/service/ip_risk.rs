use std::net::IpAddr;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use sea_orm::*;
use serde_json::Value as JsonValue;

use crate::config::IpQualityConfig;
use crate::entity::ip_risk_cache;
use crate::service::geoip::{GeoIpService, is_private};
use serverbee_common::protocol::IpQualitySnapshotData;

// ---------------------------------------------------------------------------
// ProviderResult — intermediate result from a single risk provider call
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ProviderResult {
    pub risk_score: Option<i32>,
    pub is_proxy: Option<bool>,
    pub is_vpn: Option<bool>,
    pub is_hosting: Option<bool>,
    /// Provider-supplied ip_type (e.g. "datacenter", "residential").
    pub ip_type: Option<String>,
    /// Raw provider JSON response (for the `providers` column).
    pub raw: JsonValue,
}

// ---------------------------------------------------------------------------
// IpRiskProvider trait
// ---------------------------------------------------------------------------

#[async_trait]
pub trait IpRiskProvider: Send + Sync {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult>;
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// GeoIP baseline
// ---------------------------------------------------------------------------

pub struct GeoBaseline {
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub asn: Option<String>,
    pub as_org: Option<String>,
}

/// Look up baseline GeoIP metadata from the local MMDB.
/// Never makes an external network call. Gracefully returns all-`None` if
/// the MMDB is not configured or the IP is private/loopback.
pub fn lookup_geoip_baseline(
    geoip: &Arc<RwLock<Option<GeoIpService>>>,
    ip: &str,
) -> GeoBaseline {
    let parsed: IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => {
            return GeoBaseline {
                country: None,
                region: None,
                city: None,
                asn: None,
                as_org: None,
            };
        }
    };

    if parsed.is_loopback() || is_private(&parsed) {
        return GeoBaseline {
            country: None,
            region: None,
            city: None,
            asn: None,
            as_org: None,
        };
    }

    let guard = geoip.read().unwrap();
    match guard.as_ref() {
        Some(service) => {
            let geo = service.lookup(parsed);
            // The DB-IP Lite Country database only provides country + region.
            // ASN and city are not available from this database and remain None.
            GeoBaseline {
                country: geo.country_code,
                region: geo.region,
                city: None,
                asn: None,
                as_org: None,
            }
        }
        None => GeoBaseline {
            country: None,
            region: None,
            city: None,
            asn: None,
            as_org: None,
        },
    }
}

// ---------------------------------------------------------------------------
// ip_type derivation
// ---------------------------------------------------------------------------

/// Derive an `ip_type` string from ASN ranges / flags.
///
/// Priority (highest wins):
/// 1. `is_hosting` → "datacenter" (most data-center ASNs also set this)
/// 2. `is_vpn`     → "hosting" (VPN services often use hosting infrastructure)
/// 3. `is_proxy`   → "datacenter"
/// 4. provider-supplied `ip_type` (pass-through)
/// 5. Fallback: "unknown"
pub fn derive_ip_type(
    is_hosting: bool,
    is_vpn: bool,
    is_proxy: bool,
    provider_ip_type: Option<&str>,
) -> String {
    if is_hosting {
        return "datacenter".to_string();
    }
    if is_vpn {
        return "hosting".to_string();
    }
    if is_proxy {
        return "datacenter".to_string();
    }
    if let Some(t) = provider_ip_type.filter(|t| !t.is_empty()) {
        return t.to_string();
    }
    "unknown".to_string()
}

// ---------------------------------------------------------------------------
// Risk level derivation
// ---------------------------------------------------------------------------

/// Derive a `risk_level` string from an optional 0-100 risk score.
///
/// - `None`       → "unknown"
/// - 0–33         → "low"
/// - 34–66        → "medium"
/// - 67+          → "high"
pub fn derive_risk_level(score: Option<i32>) -> String {
    match score {
        None => "unknown".to_string(),
        Some(s) if s < 34 => "low".to_string(),
        Some(s) if s < 67 => "medium".to_string(),
        _ => "high".to_string(),
    }
}

// ---------------------------------------------------------------------------
// IpRiskService
// ---------------------------------------------------------------------------

pub struct IpRiskService {
    pub config: IpQualityConfig,
}

impl IpRiskService {
    pub fn new(config: IpQualityConfig) -> Self {
        Self { config }
    }

    /// Score an IP address:
    /// 1. Check `ip_risk_cache` — if fresh (< 24h), return the cached snapshot.
    /// 2. On cache miss / expired: look up GeoIP baseline + call provider (if configured).
    /// 3. Upsert `ip_risk_cache`.
    /// 4. Return the `IpQualitySnapshotData`.
    ///
    /// Provider failures are non-fatal: the snapshot is returned with
    /// GeoIP-baseline data and `risk_score = None`.
    pub async fn score_ip(
        &self,
        db: &DatabaseConnection,
        geoip: &Arc<RwLock<Option<GeoIpService>>>,
        ip: &str,
    ) -> IpQualitySnapshotData {
        // 1. Check cache
        if let Some(snapshot) = self.read_cache(db, ip).await {
            return snapshot;
        }

        // 2. GeoIP baseline (always local, never fails hard)
        let baseline = lookup_geoip_baseline(geoip, ip);

        // 3. Provider lookup (opt-in, non-fatal)
        let provider = provider_for_config(&self.config);
        let provider_result = if let Some(p) = provider {
            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                p.lookup(ip),
            )
            .await
            {
                Ok(Ok(result)) => Some(result),
                Ok(Err(e)) => {
                    tracing::warn!("IP risk provider {} failed for {ip}: {e}", p.name());
                    None
                }
                Err(_) => {
                    tracing::warn!("IP risk provider timed out for {ip}");
                    None
                }
            }
        } else {
            None
        };

        // 4. Merge
        let (risk_score, is_proxy, is_vpn, is_hosting, provider_ip_type, providers_json) =
            match &provider_result {
                Some(r) => {
                    let providers_json = serde_json::json!({
                        self.config.risk_provider.clone(): r.raw
                    });
                    (
                        r.risk_score,
                        r.is_proxy.unwrap_or(false),
                        r.is_vpn.unwrap_or(false),
                        r.is_hosting.unwrap_or(false),
                        r.ip_type.clone(),
                        providers_json,
                    )
                }
                None => (None, false, false, false, None, serde_json::json!({})),
            };

        let ip_type = derive_ip_type(
            is_hosting,
            is_vpn,
            is_proxy,
            provider_ip_type.as_deref(),
        );
        let risk_level = derive_risk_level(risk_score);

        let now = Utc::now();
        let snapshot = IpQualitySnapshotData {
            ip: ip.to_string(),
            asn: baseline.asn,
            as_org: baseline.as_org,
            country: baseline.country,
            region: baseline.region,
            city: baseline.city,
            ip_type,
            is_proxy,
            is_vpn,
            is_hosting,
            risk_score,
            risk_level,
            checked_at: now,
        };

        // 5. Upsert cache
        self.write_cache(db, &snapshot, &providers_json).await;

        snapshot
    }

    // -----------------------------------------------------------------------
    // Cache helpers
    // -----------------------------------------------------------------------

    /// Read a cache row if it exists and is fresher than 24 hours.
    async fn read_cache(
        &self,
        db: &DatabaseConnection,
        ip: &str,
    ) -> Option<IpQualitySnapshotData> {
        let row = ip_risk_cache::Entity::find_by_id(ip)
            .one(db)
            .await
            .ok()
            .flatten()?;

        let age = Utc::now() - row.checked_at;
        if age > Duration::hours(24) {
            return None;
        }

        Some(IpQualitySnapshotData {
            ip: row.ip,
            asn: row.asn,
            as_org: row.as_org,
            country: row.country,
            region: row.region,
            city: row.city,
            ip_type: row.ip_type,
            is_proxy: row.is_proxy,
            is_vpn: row.is_vpn,
            is_hosting: row.is_hosting,
            risk_score: row.risk_score,
            risk_level: row.risk_level,
            checked_at: row.checked_at,
        })
    }

    /// Upsert a scored result into `ip_risk_cache`.
    async fn write_cache(
        &self,
        db: &DatabaseConnection,
        snapshot: &IpQualitySnapshotData,
        providers_json: &JsonValue,
    ) {
        let providers_str = providers_json.to_string();

        let sql = "INSERT OR REPLACE INTO ip_risk_cache \
            (ip, asn, as_org, country, region, city, ip_type, is_proxy, is_vpn, is_hosting, \
             risk_score, risk_level, providers, checked_at) \
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

        let opt_str = |s: &Option<String>| -> sea_orm::Value {
            match s {
                Some(v) => sea_orm::Value::String(Some(Box::new(v.clone()))),
                None => sea_orm::Value::String(None),
            }
        };
        let opt_int = |v: Option<i32>| -> sea_orm::Value {
            match v {
                Some(n) => sea_orm::Value::Int(Some(n)),
                None => sea_orm::Value::Int(None),
            }
        };

        let result = db
            .execute(Statement::from_sql_and_values(
                DatabaseBackend::Sqlite,
                sql,
                vec![
                    sea_orm::Value::String(Some(Box::new(snapshot.ip.clone()))),
                    opt_str(&snapshot.asn),
                    opt_str(&snapshot.as_org),
                    opt_str(&snapshot.country),
                    opt_str(&snapshot.region),
                    opt_str(&snapshot.city),
                    sea_orm::Value::String(Some(Box::new(snapshot.ip_type.clone()))),
                    sea_orm::Value::Int(Some(snapshot.is_proxy as i32)),
                    sea_orm::Value::Int(Some(snapshot.is_vpn as i32)),
                    sea_orm::Value::Int(Some(snapshot.is_hosting as i32)),
                    opt_int(snapshot.risk_score),
                    sea_orm::Value::String(Some(Box::new(snapshot.risk_level.clone()))),
                    sea_orm::Value::String(Some(Box::new(providers_str))),
                    sea_orm::Value::String(Some(Box::new(snapshot.checked_at.to_rfc3339()))),
                ],
            ))
            .await;

        if let Err(e) = result {
            tracing::warn!("Failed to upsert ip_risk_cache: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------

pub fn provider_for_config(config: &IpQualityConfig) -> Option<Box<dyn IpRiskProvider>> {
    match config.risk_provider.as_str() {
        "scamalytics" => config.scamalytics.as_ref().map(|k| {
            Box::new(ScamalyticsProvider {
                api_key: k.api_key.clone(),
            }) as Box<dyn IpRiskProvider>
        }),
        "ipqs" => config.ipqs.as_ref().map(|k| {
            Box::new(IpQualityScoreProvider {
                api_key: k.api_key.clone(),
            }) as Box<dyn IpRiskProvider>
        }),
        "proxycheck" => config.proxycheck.as_ref().map(|k| {
            Box::new(ProxyCheckProvider {
                api_key: k.api_key.clone(),
            }) as Box<dyn IpRiskProvider>
        }),
        "abuseipdb" => config.abuseipdb.as_ref().map(|k| {
            Box::new(AbuseIpdbProvider {
                api_key: k.api_key.clone(),
            }) as Box<dyn IpRiskProvider>
        }),
        "ip-api" => Some(Box::new(IpApiProvider) as Box<dyn IpRiskProvider>),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Provider: Scamalytics
// ---------------------------------------------------------------------------

pub struct ScamalyticsProvider {
    pub api_key: String,
}

impl ScamalyticsProvider {
    pub fn parse_response(json: &str) -> ProviderResult {
        let v: JsonValue = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return ProviderResult::default(),
        };

        let risk_score = v
            .get("score")
            .and_then(|s| s.as_str())
            .and_then(|s| s.parse::<i32>().ok())
            .or_else(|| v.get("score").and_then(|s| s.as_i64()).map(|n| n as i32));

        let is_proxy = v
            .get("proxy")
            .or_else(|| v.get("is_proxy"))
            .and_then(|b| b.as_bool())
            .or_else(|| {
                v.get("proxy")
                    .and_then(|s| s.as_str())
                    .map(|s| s == "yes" || s == "1" || s == "true")
            });
        let is_vpn = v
            .get("vpn")
            .or_else(|| v.get("is_vpn"))
            .and_then(|b| b.as_bool())
            .or_else(|| {
                v.get("vpn")
                    .and_then(|s| s.as_str())
                    .map(|s| s == "yes" || s == "1" || s == "true")
            });
        let is_hosting = v
            .get("hosting")
            .or_else(|| v.get("is_hosting"))
            .and_then(|b| b.as_bool())
            .or_else(|| {
                v.get("hosting")
                    .and_then(|s| s.as_str())
                    .map(|s| s == "yes" || s == "1" || s == "true")
            });

        ProviderResult {
            risk_score,
            is_proxy,
            is_vpn,
            is_hosting,
            ip_type: None,
            raw: v,
        }
    }
}

#[async_trait]
impl IpRiskProvider for ScamalyticsProvider {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let url = format!("https://api11.scamalytics.com/ip/?key={}&ip={}", self.api_key, ip);
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(Self::parse_response(&body))
    }

    fn name(&self) -> &'static str {
        "scamalytics"
    }
}

// ---------------------------------------------------------------------------
// Provider: IPQualityScore
// ---------------------------------------------------------------------------

pub struct IpQualityScoreProvider {
    pub api_key: String,
}

impl IpQualityScoreProvider {
    pub fn parse_response(json: &str) -> ProviderResult {
        let v: JsonValue = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return ProviderResult::default(),
        };

        let risk_score = v
            .get("fraud_score")
            .and_then(|s| s.as_i64())
            .map(|n| n as i32);

        let is_proxy = v.get("proxy").and_then(|b| b.as_bool());
        let is_vpn = v.get("vpn").and_then(|b| b.as_bool());
        let is_hosting = v
            .get("host")
            .and_then(|b| b.as_bool())
            .or_else(|| v.get("is_crawler").and_then(|b| b.as_bool()));
        let ip_type = v.get("connection_type").and_then(|s| s.as_str()).map(|s| {
            match s.to_lowercase().as_str() {
                "residential" => "residential".to_string(),
                "mobile" => "mobile".to_string(),
                "corporate" => "isp".to_string(),
                "data_center" | "datacenter" => "datacenter".to_string(),
                _ => s.to_string(),
            }
        });

        ProviderResult {
            risk_score,
            is_proxy,
            is_vpn,
            is_hosting,
            ip_type,
            raw: v,
        }
    }
}

#[async_trait]
impl IpRiskProvider for IpQualityScoreProvider {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let url = format!(
            "https://ipqualityscore.com/api/json/ip/{}/{ip}",
            self.api_key
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(Self::parse_response(&body))
    }

    fn name(&self) -> &'static str {
        "ipqs"
    }
}

// ---------------------------------------------------------------------------
// Provider: ProxyCheck
// ---------------------------------------------------------------------------

pub struct ProxyCheckProvider {
    pub api_key: String,
}

impl ProxyCheckProvider {
    pub fn parse_response(json: &str) -> ProviderResult {
        let v: JsonValue = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return ProviderResult::default(),
        };

        // proxycheck.io response: { "status": "ok", "<ip>": { ... } }
        // Find the IP entry by looking for the first non-"status" key.
        let ip_data = v
            .as_object()
            .and_then(|obj| obj.values().find(|val| val.is_object()));

        let Some(ip_data) = ip_data else {
            return ProviderResult { raw: v, ..Default::default() };
        };

        let risk_score = ip_data
            .get("risk")
            .and_then(|s| s.as_i64())
            .map(|n| n as i32);

        let is_proxy = ip_data
            .get("proxy")
            .and_then(|s| s.as_str())
            .map(|s| s == "yes");
        let is_vpn = ip_data
            .get("vpn")
            .and_then(|b| b.as_bool())
            .or_else(|| {
                ip_data
                    .get("type")
                    .and_then(|s| s.as_str())
                    .map(|s| s.eq_ignore_ascii_case("vpn"))
            });
        let ip_type = ip_data
            .get("type")
            .and_then(|s| s.as_str())
            .map(|s| s.to_lowercase());

        ProviderResult {
            risk_score,
            is_proxy,
            is_vpn,
            is_hosting: None,
            ip_type,
            raw: v,
        }
    }
}

#[async_trait]
impl IpRiskProvider for ProxyCheckProvider {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let url = format!(
            "https://proxycheck.io/v2/{ip}?key={}&vpn=1&risk=1",
            self.api_key
        );
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(Self::parse_response(&body))
    }

    fn name(&self) -> &'static str {
        "proxycheck"
    }
}

// ---------------------------------------------------------------------------
// Provider: AbuseIPDB
// ---------------------------------------------------------------------------

pub struct AbuseIpdbProvider {
    pub api_key: String,
}

impl AbuseIpdbProvider {
    pub fn parse_response(json: &str) -> ProviderResult {
        let v: JsonValue = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return ProviderResult::default(),
        };

        let data = v.get("data");

        let risk_score = data
            .and_then(|d| d.get("abuseConfidenceScore"))
            .and_then(|s| s.as_i64())
            .map(|n| n as i32);

        let is_hosting = data
            .and_then(|d| d.get("isp"))
            .and_then(|s| s.as_str())
            .map(|s| {
                let lower = s.to_lowercase();
                lower.contains("hosting")
                    || lower.contains("cloud")
                    || lower.contains("datacenter")
                    || lower.contains("data center")
            });

        let ip_type = data
            .and_then(|d| d.get("usageType"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_lowercase());

        ProviderResult {
            risk_score,
            is_proxy: None,
            is_vpn: None,
            is_hosting,
            ip_type,
            raw: v,
        }
    }
}

#[async_trait]
impl IpRiskProvider for AbuseIpdbProvider {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client
            .get("https://api.abuseipdb.com/api/v2/check")
            .header("Key", &self.api_key)
            .header("Accept", "application/json")
            .query(&[("ipAddress", ip), ("maxAgeInDays", "30")])
            .send()
            .await?;
        let body = resp.text().await?;
        Ok(Self::parse_response(&body))
    }

    fn name(&self) -> &'static str {
        "abuseipdb"
    }
}

// ---------------------------------------------------------------------------
// Provider: ip-api.com
//
// NOTE: ip-api.com's free JSON endpoint is HTTP-only (not HTTPS). It also
// prohibits commercial use on the free tier. Enable only for non-commercial
// deployments and ensure you understand the license terms at https://ip-api.com/docs/legal.
// ---------------------------------------------------------------------------

pub struct IpApiProvider;

impl IpApiProvider {
    pub fn parse_response(json: &str) -> ProviderResult {
        let v: JsonValue = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return ProviderResult::default(),
        };

        // ip-api.com does not provide a fraud/risk score.
        // It does provide proxy/hosting info in the pro tier, but the free
        // endpoint does not. We map available fields conservatively.
        let is_hosting = v
            .get("hosting")
            .and_then(|b| b.as_bool());
        let is_proxy = v
            .get("proxy")
            .and_then(|b| b.as_bool());

        let ip_type = if is_hosting.unwrap_or(false) || is_proxy.unwrap_or(false) {
            Some("datacenter".to_string())
        } else {
            v.get("org")
                .and_then(|s| s.as_str())
                .map(|_| "isp".to_string())
        };

        ProviderResult {
            risk_score: None,
            is_proxy,
            is_vpn: None,
            is_hosting,
            ip_type,
            raw: v,
        }
    }
}

#[async_trait]
impl IpRiskProvider for IpApiProvider {
    async fn lookup(&self, ip: &str) -> anyhow::Result<ProviderResult> {
        // NOTE: HTTP only — ip-api.com does not support HTTPS on the free tier.
        let url = format!("http://ip-api.com/json/{ip}?fields=status,message,country,countryCode,region,regionName,city,zip,lat,lon,timezone,isp,org,as,hosting,proxy,query");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(&url).send().await?;
        let body = resp.text().await?;
        Ok(Self::parse_response(&body))
    }

    fn name(&self) -> &'static str {
        "ip-api"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IpQualityConfig;
    use crate::test_utils::setup_test_db;

    // -----------------------------------------------------------------------
    // Task 10 tests: derive_ip_type, trait dispatch, score_ip with "none"
    // -----------------------------------------------------------------------

    #[test]
    fn derive_ip_type_hosting_wins() {
        assert_eq!(derive_ip_type(true, false, false, None), "datacenter");
    }

    #[test]
    fn derive_ip_type_vpn_maps_to_hosting() {
        assert_eq!(derive_ip_type(false, true, false, None), "hosting");
    }

    #[test]
    fn derive_ip_type_proxy_maps_to_datacenter() {
        assert_eq!(derive_ip_type(false, false, true, None), "datacenter");
    }

    #[test]
    fn derive_ip_type_provider_passthrough() {
        assert_eq!(
            derive_ip_type(false, false, false, Some("residential")),
            "residential"
        );
    }

    #[test]
    fn derive_ip_type_fallback_unknown() {
        assert_eq!(derive_ip_type(false, false, false, None), "unknown");
    }

    #[test]
    fn derive_ip_type_hosting_overrides_vpn() {
        // is_hosting takes priority over is_vpn
        assert_eq!(derive_ip_type(true, true, false, None), "datacenter");
    }

    #[test]
    fn derive_risk_level_none_is_unknown() {
        assert_eq!(derive_risk_level(None), "unknown");
    }

    #[test]
    fn derive_risk_level_thresholds() {
        assert_eq!(derive_risk_level(Some(0)), "low");
        assert_eq!(derive_risk_level(Some(33)), "low");
        assert_eq!(derive_risk_level(Some(34)), "medium");
        assert_eq!(derive_risk_level(Some(66)), "medium");
        assert_eq!(derive_risk_level(Some(67)), "high");
        assert_eq!(derive_risk_level(Some(100)), "high");
    }

    // Verify IpRiskProvider is object-safe (compiles as Box<dyn IpRiskProvider>)
    #[test]
    fn provider_trait_is_object_safe() {
        let _: Option<Box<dyn IpRiskProvider>> = None;
    }

    // provider_for_config returns None when risk_provider = "none"
    #[test]
    fn provider_for_config_none() {
        let cfg = IpQualityConfig::default();
        assert!(provider_for_config(&cfg).is_none());
    }

    // provider_for_config returns Some when a provider + key is configured
    #[test]
    fn provider_for_config_scamalytics() {
        let cfg = IpQualityConfig {
            risk_provider: "scamalytics".to_string(),
            scamalytics: Some(crate::config::RiskProviderKey {
                api_key: "test_key".to_string(),
            }),
            ..Default::default()
        };
        let provider = provider_for_config(&cfg);
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name(), "scamalytics");
    }

    #[tokio::test]
    async fn score_ip_no_provider_returns_geoip_baseline() {
        let (db, _tmp) = setup_test_db().await;
        let cfg = IpQualityConfig::default(); // risk_provider = "none"
        let service = IpRiskService::new(cfg);
        let geoip: Arc<RwLock<Option<GeoIpService>>> = Arc::new(RwLock::new(None));

        let result = service.score_ip(&db, &geoip, "1.2.3.4").await;

        assert_eq!(result.ip, "1.2.3.4");
        assert!(result.risk_score.is_none());
        assert_eq!(result.risk_level, "unknown");
        // GeoIP fields are None when MMDB is not configured
        assert!(result.country.is_none());
        assert!(result.asn.is_none());
    }

    // -----------------------------------------------------------------------
    // Task 11 tests: cache read-through + provider parse fixtures
    // -----------------------------------------------------------------------

    struct MockProvider {
        pub call_count: Arc<std::sync::atomic::AtomicUsize>,
        pub result: ProviderResult,
    }

    impl MockProvider {
        fn new(result: ProviderResult) -> (Self, Arc<std::sync::atomic::AtomicUsize>) {
            let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let provider = Self {
                call_count: count.clone(),
                result,
            };
            (provider, count)
        }
    }

    #[async_trait]
    impl IpRiskProvider for MockProvider {
        async fn lookup(&self, _ip: &str) -> anyhow::Result<ProviderResult> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.result.clone())
        }

        fn name(&self) -> &'static str {
            "mock"
        }
    }

    /// Insert a cache row with a given `checked_at` timestamp.
    async fn insert_cache_row(db: &DatabaseConnection, ip: &str, checked_at: chrono::DateTime<Utc>) {
        let sql = "INSERT OR REPLACE INTO ip_risk_cache \
            (ip, asn, as_org, country, region, city, ip_type, is_proxy, is_vpn, is_hosting, \
             risk_score, risk_level, providers, checked_at) \
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
        db.execute(Statement::from_sql_and_values(
            DatabaseBackend::Sqlite,
            sql,
            vec![
                ip.into(),
                sea_orm::Value::String(None),
                sea_orm::Value::String(None),
                sea_orm::Value::String(None),
                sea_orm::Value::String(None),
                sea_orm::Value::String(None),
                "unknown".into(),
                0i32.into(),
                0i32.into(),
                0i32.into(),
                sea_orm::Value::Int(None),
                "unknown".into(),
                "{}".into(),
                checked_at.to_rfc3339().into(),
            ],
        ))
        .await
        .unwrap();
    }

    /// Wrapper that uses a custom provider for testing the cache layer.
    async fn score_ip_with_provider(
        service: &IpRiskService,
        db: &DatabaseConnection,
        ip: &str,
        provider: Option<Box<dyn IpRiskProvider>>,
    ) -> IpQualitySnapshotData {
        // Check cache first
        if let Some(snapshot) = service.read_cache(db, ip).await {
            return snapshot;
        }

        let geoip: Arc<RwLock<Option<GeoIpService>>> = Arc::new(RwLock::new(None));
        let baseline = lookup_geoip_baseline(&geoip, ip);

        let provider_result = if let Some(p) = provider {
            match tokio::time::timeout(
                std::time::Duration::from_secs(15),
                p.lookup(ip),
            )
            .await
            {
                Ok(Ok(result)) => Some(result),
                Ok(Err(_)) | Err(_) => None,
            }
        } else {
            None
        };

        let (risk_score, is_proxy, is_vpn, is_hosting, provider_ip_type, providers_json) =
            match &provider_result {
                Some(r) => {
                    let providers_json = serde_json::json!({ "mock": r.raw });
                    (
                        r.risk_score,
                        r.is_proxy.unwrap_or(false),
                        r.is_vpn.unwrap_or(false),
                        r.is_hosting.unwrap_or(false),
                        r.ip_type.clone(),
                        providers_json,
                    )
                }
                None => (None, false, false, false, None, serde_json::json!({})),
            };

        let ip_type = derive_ip_type(is_hosting, is_vpn, is_proxy, provider_ip_type.as_deref());
        let risk_level = derive_risk_level(risk_score);

        let snapshot = IpQualitySnapshotData {
            ip: ip.to_string(),
            asn: baseline.asn,
            as_org: baseline.as_org,
            country: baseline.country,
            region: baseline.region,
            city: baseline.city,
            ip_type,
            is_proxy,
            is_vpn,
            is_hosting,
            risk_score,
            risk_level,
            checked_at: Utc::now(),
        };

        service.write_cache(db, &snapshot, &providers_json).await;
        snapshot
    }

    #[tokio::test]
    async fn cache_hit_skips_provider_call() {
        let (db, _tmp) = setup_test_db().await;
        let cfg = IpQualityConfig::default();
        let service = IpRiskService::new(cfg);

        let fresh_time = Utc::now() - Duration::hours(1); // 1h ago — fresh
        insert_cache_row(&db, "5.6.7.8", fresh_time).await;

        let (mock, call_count) = MockProvider::new(ProviderResult::default());

        let _result = score_ip_with_provider(
            &service,
            &db,
            "5.6.7.8",
            Some(Box::new(mock)),
        )
        .await;

        assert_eq!(
            call_count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "provider should not be called on cache hit"
        );
    }

    #[tokio::test]
    async fn cache_miss_calls_provider_and_upserts() {
        let (db, _tmp) = setup_test_db().await;
        let cfg = IpQualityConfig::default();
        let service = IpRiskService::new(cfg);

        let (mock, call_count) = MockProvider::new(ProviderResult {
            risk_score: Some(42),
            ..Default::default()
        });

        let result = score_ip_with_provider(
            &service,
            &db,
            "9.10.11.12",
            Some(Box::new(mock)),
        )
        .await;

        assert_eq!(
            call_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "provider should be called on cache miss"
        );
        assert_eq!(result.risk_score, Some(42));

        // Second call — should hit the cache
        let (mock2, call_count2) = MockProvider::new(ProviderResult::default());
        let _result2 = score_ip_with_provider(
            &service,
            &db,
            "9.10.11.12",
            Some(Box::new(mock2)),
        )
        .await;
        assert_eq!(
            call_count2.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "provider should not be called on cache hit after upsert"
        );
    }

    #[tokio::test]
    async fn expired_cache_calls_provider() {
        let (db, _tmp) = setup_test_db().await;
        let cfg = IpQualityConfig::default();
        let service = IpRiskService::new(cfg);

        // Insert a row that is 25 hours old (expired)
        let expired_time = Utc::now() - Duration::hours(25);
        insert_cache_row(&db, "13.14.15.16", expired_time).await;

        let (mock, call_count) = MockProvider::new(ProviderResult {
            risk_score: Some(77),
            ..Default::default()
        });

        let result = score_ip_with_provider(
            &service,
            &db,
            "13.14.15.16",
            Some(Box::new(mock)),
        )
        .await;

        assert_eq!(
            call_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "provider should be called when cache is expired"
        );
        assert_eq!(result.risk_score, Some(77));
    }

    // -----------------------------------------------------------------------
    // Provider parse tests using recorded fixture JSON (no live network)
    // -----------------------------------------------------------------------

    #[test]
    fn scamalytics_parse_high_risk() {
        let json = r#"{
            "status": "ok",
            "score": "87",
            "risk": "high",
            "proxy": "yes",
            "vpn": "yes",
            "hosting": "no"
        }"#;
        let result = ScamalyticsProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(87));
        assert_eq!(result.is_proxy, Some(true));
        assert_eq!(result.is_vpn, Some(true));
        assert_eq!(result.is_hosting, Some(false));
    }

    #[test]
    fn scamalytics_parse_clean_ip() {
        let json = r#"{
            "status": "ok",
            "score": "2",
            "risk": "very low",
            "proxy": "no",
            "vpn": "no",
            "hosting": "no"
        }"#;
        let result = ScamalyticsProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(2));
        assert_eq!(result.is_proxy, Some(false));
        assert_eq!(result.is_vpn, Some(false));
    }

    #[test]
    fn ipqs_parse_fraud_score() {
        let json = r#"{
            "success": true,
            "message": "Success",
            "fraud_score": 75,
            "proxy": true,
            "vpn": false,
            "host": true,
            "connection_type": "Data_Center"
        }"#;
        let result = IpQualityScoreProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(75));
        assert_eq!(result.is_proxy, Some(true));
        assert_eq!(result.is_vpn, Some(false));
        assert_eq!(result.is_hosting, Some(true));
        assert!(result.ip_type.is_some());
    }

    #[test]
    fn ipqs_parse_residential() {
        let json = r#"{
            "success": true,
            "fraud_score": 10,
            "proxy": false,
            "vpn": false,
            "host": false,
            "connection_type": "Residential"
        }"#;
        let result = IpQualityScoreProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(10));
        assert_eq!(result.ip_type, Some("residential".to_string()));
    }

    #[test]
    fn proxycheck_parse_proxy() {
        let json = r#"{
            "status": "ok",
            "1.2.3.4": {
                "proxy": "yes",
                "type": "VPN",
                "risk": 88
            }
        }"#;
        let result = ProxyCheckProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(88));
        assert_eq!(result.is_proxy, Some(true));
    }

    #[test]
    fn proxycheck_parse_clean() {
        let json = r#"{
            "status": "ok",
            "8.8.8.8": {
                "proxy": "no",
                "risk": 0
            }
        }"#;
        let result = ProxyCheckProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(0));
        assert_eq!(result.is_proxy, Some(false));
    }

    #[test]
    fn abuseipdb_parse_high_abuse() {
        let json = r#"{
            "data": {
                "ipAddress": "1.2.3.4",
                "isPublic": true,
                "abuseConfidenceScore": 92,
                "isp": "Digital Ocean Hosting",
                "usageType": "Data Center/Web Hosting/Transit"
            }
        }"#;
        let result = AbuseIpdbProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(92));
        assert_eq!(result.is_hosting, Some(true));
        assert!(result.ip_type.is_some());
    }

    #[test]
    fn abuseipdb_parse_residential() {
        let json = r#"{
            "data": {
                "ipAddress": "9.10.11.12",
                "isPublic": true,
                "abuseConfidenceScore": 5,
                "isp": "Comcast ISP",
                "usageType": "Fixed Line ISP"
            }
        }"#;
        let result = AbuseIpdbProvider::parse_response(json);
        assert_eq!(result.risk_score, Some(5));
        assert_eq!(result.is_hosting, Some(false));
    }

    #[test]
    fn ipapi_parse_hosting() {
        let json = r#"{
            "status": "success",
            "country": "United States",
            "countryCode": "US",
            "hosting": true,
            "proxy": false,
            "org": "AS14061 DigitalOcean",
            "query": "1.2.3.4"
        }"#;
        let result = IpApiProvider::parse_response(json);
        assert_eq!(result.is_hosting, Some(true));
        assert_eq!(result.ip_type, Some("datacenter".to_string()));
    }

    #[test]
    fn ipapi_parse_residential() {
        let json = r#"{
            "status": "success",
            "country": "Germany",
            "countryCode": "DE",
            "hosting": false,
            "proxy": false,
            "org": "AS1234 Deutsche Telekom",
            "query": "5.6.7.8"
        }"#;
        let result = IpApiProvider::parse_response(json);
        assert_eq!(result.is_hosting, Some(false));
        assert_eq!(result.ip_type, Some("isp".to_string()));
    }

    #[test]
    fn scamalytics_parse_invalid_json() {
        let result = ScamalyticsProvider::parse_response("not json");
        assert!(result.risk_score.is_none());
    }

    #[test]
    fn ipqs_parse_invalid_json() {
        let result = IpQualityScoreProvider::parse_response("{}");
        assert!(result.risk_score.is_none());
    }
}
