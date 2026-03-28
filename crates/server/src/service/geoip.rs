use std::io::Read;
use std::net::IpAddr;
use std::path::Path;

use chrono::Datelike;
use maxminddb::Reader;
use serde::Deserialize;

/// GeoIP lookup result
pub struct GeoLookup {
    pub country_code: Option<String>,
    pub region: Option<String>,
}

/// Thread-safe GeoIP reader backed by MaxMind MMDB.
pub struct GeoIpService {
    reader: Reader<Vec<u8>>,
    /// Which file this was loaded from (for status endpoint).
    pub source_path: String,
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

/// Default filename for the downloaded DB-IP Lite Country database.
pub const DBIP_FILENAME: &str = "dbip-country-lite.mmdb";

impl GeoIpService {
    /// Load from a file path. Returns None if file doesn't exist or is invalid.
    pub fn load(mmdb_path: &str) -> Option<Self> {
        if mmdb_path.is_empty() || !Path::new(mmdb_path).exists() {
            return None;
        }

        match Reader::open_readfile(mmdb_path) {
            Ok(reader) => {
                tracing::info!("GeoIP MMDB loaded from {mmdb_path}");
                Some(Self {
                    reader,
                    source_path: mmdb_path.to_string(),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load GeoIP MMDB from {mmdb_path}: {e}");
                None
            }
        }
    }

    /// Load from in-memory bytes (used after download + decompress).
    pub fn load_from_bytes(bytes: Vec<u8>, source_path: String) -> Result<Self, String> {
        Reader::from_source(bytes)
            .map(|reader| Self { reader, source_path })
            .map_err(|e| format!("Invalid MMDB data: {e}"))
    }

    /// Lookup an IP address and return country/region info.
    pub fn lookup(&self, ip: IpAddr) -> GeoLookup {
        if ip.is_loopback() || is_private(&ip) {
            return GeoLookup {
                country_code: None,
                region: None,
            };
        }

        match self.reader.lookup(ip) {
            Ok(result) => match result.decode::<GeoCity>() {
                Ok(Some(city)) => {
                    let country_code = city.country.and_then(|c| c.iso_code);
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
                Ok(None) => GeoLookup {
                    country_code: None,
                    region: None,
                },
                Err(e) => {
                    tracing::debug!("GeoIP decode failed for {ip}: {e}");
                    GeoLookup {
                        country_code: None,
                        region: None,
                    }
                }
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

/// Download DB-IP Lite Country MMDB, decompress, save to data_dir, return loaded service.
pub async fn download_dbip(data_dir: &str) -> Result<GeoIpService, String> {
    let now = chrono::Utc::now();
    let url = format!(
        "https://download.db-ip.com/free/dbip-country-lite-{}-{:02}.mmdb.gz",
        now.year(),
        now.month()
    );
    tracing::info!("Downloading GeoIP database from {url}");

    let resp = reqwest::get(&url)
        .await
        .map_err(|e| format!("Failed to download: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Failed to download: server returned {}", resp.status()));
    }

    let compressed = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    // Decompress gzip
    let mut decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(&compressed));
    let mut decompressed = Vec::new();
    decoder
        .read_to_end(&mut decompressed)
        .map_err(|e| format!("Failed to decompress: {e}"))?;

    // Validate it's a valid MMDB before writing to disk
    let final_path = Path::new(data_dir).join(DBIP_FILENAME);
    let service = GeoIpService::load_from_bytes(decompressed.clone(), final_path.display().to_string())?;

    // Atomic write: tmp file then rename
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create data directory: {e}"))?;
    let tmp_path = Path::new(data_dir).join(format!("{DBIP_FILENAME}.tmp"));
    std::fs::write(&tmp_path, &decompressed)
        .map_err(|e| format!("Failed to write database: {e}"))?;
    std::fs::rename(&tmp_path, &final_path)
        .map_err(|e| format!("Failed to save database: {e}"))?;

    tracing::info!("GeoIP database saved to {}", final_path.display());
    Ok(service)
}

fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_returns_none() {
        assert!(GeoIpService::load("").is_none());
        assert!(GeoIpService::load("/nonexistent/path.mmdb").is_none());
    }

    #[test]
    fn test_load_from_bytes_invalid_data() {
        let result = GeoIpService::load_from_bytes(vec![0, 1, 2, 3], "test.mmdb".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_is_private_ipv4() {
        // Private ranges
        assert!(is_private(&"192.168.1.1".parse().unwrap()));
        assert!(is_private(&"10.0.0.1".parse().unwrap()));
        assert!(is_private(&"172.16.0.1".parse().unwrap()));
        assert!(is_private(&"172.31.255.255".parse().unwrap()));

        // Link-local
        assert!(is_private(&"169.254.1.1".parse().unwrap()));

        // Public
        assert!(!is_private(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private(&"1.1.1.1".parse().unwrap()));
        assert!(!is_private(&"203.0.113.1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ipv6() {
        // IPv6 always returns false in current implementation
        assert!(!is_private(&"::1".parse().unwrap()));
        assert!(!is_private(&"fe80::1".parse().unwrap()));
        assert!(!is_private(&"2001:db8::1".parse().unwrap()));
    }
}
