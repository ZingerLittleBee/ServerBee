use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use maxminddb::{MaxMindDBError, Reader};
use serde::Deserialize;

/// GeoIP lookup result
pub struct GeoLookup {
    pub country_code: Option<String>,
    pub region: Option<String>,
}

/// Thread-safe GeoIP reader backed by MaxMind MMDB.
pub struct GeoIpService {
    reader: Arc<Reader<Vec<u8>>>,
}

#[derive(Deserialize)]
struct GeoCity {
    country: Option<GeoCountry>,
    subdivisions: Option<Vec<GeoSubdivision>>,
    city: Option<GeoCityNames>,
}

#[derive(Deserialize)]
struct GeoCountry {
    iso_code: Option<String>,
}

#[derive(Deserialize)]
struct GeoSubdivision {
    names: Option<std::collections::BTreeMap<String, String>>,
}

#[derive(Deserialize)]
struct GeoCityNames {
    names: Option<std::collections::BTreeMap<String, String>>,
}

impl GeoIpService {
    /// Load a MaxMind MMDB file. Returns None if the file doesn't exist.
    pub fn load(mmdb_path: &str) -> Option<Self> {
        if mmdb_path.is_empty() || !Path::new(mmdb_path).exists() {
            tracing::warn!("GeoIP MMDB file not found at: {mmdb_path}");
            return None;
        }

        match Reader::open_readfile(mmdb_path) {
            Ok(reader) => {
                tracing::info!("GeoIP MMDB loaded from {mmdb_path}");
                Some(Self {
                    reader: Arc::new(reader),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load GeoIP MMDB: {e}");
                None
            }
        }
    }

    /// Lookup an IP address and return country/region info.
    pub fn lookup(&self, ip: IpAddr) -> GeoLookup {
        // Skip private/loopback addresses
        if ip.is_loopback() || is_private(&ip) {
            return GeoLookup {
                country_code: None,
                region: None,
            };
        }

        match self.reader.lookup::<GeoCity>(ip) {
            Ok(city) => {
                let country_code = city
                    .country
                    .and_then(|c| c.iso_code);

                // Prefer city name, fall back to first subdivision name
                let region = city
                    .city
                    .and_then(|c| c.names)
                    .and_then(|n| n.get("en").cloned())
                    .or_else(|| {
                        city.subdivisions
                            .and_then(|subs| subs.into_iter().next())
                            .and_then(|s| s.names)
                            .and_then(|n| n.get("en").cloned())
                    });

                GeoLookup {
                    country_code,
                    region,
                }
            }
            Err(MaxMindDBError::AddressNotFoundError(_)) => GeoLookup {
                country_code: None,
                region: None,
            },
            Err(e) => {
                tracing::debug!("GeoIP lookup failed for {ip}: {e}");
                GeoLookup {
                    country_code: None,
                    region: None,
                }
            }
        }
    }
}

fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(_) => false, // Simplified — IPv6 private detection is complex
    }
}
