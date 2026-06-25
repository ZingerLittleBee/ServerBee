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
            .map(|reader| Self {
                reader,
                source_path,
            })
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
        return Err(format!(
            "Failed to download: server returned {}",
            resp.status()
        ));
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
    let service =
        GeoIpService::load_from_bytes(decompressed.clone(), final_path.display().to_string())?;

    // Atomic write: tmp file then rename
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create data directory: {e}"))?;
    let tmp_path = Path::new(data_dir).join(format!("{DBIP_FILENAME}.tmp"));
    std::fs::write(&tmp_path, &decompressed)
        .map_err(|e| format!("Failed to write database: {e}"))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| format!("Failed to save database: {e}"))?;

    tracing::info!("GeoIP database saved to {}", final_path.display());
    Ok(service)
}

pub(crate) fn is_private(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            // Unique local fc00::/7 and link-local fe80::/10 are non-routable
            // and produce no useful GeoIP result.
            let seg = v6.segments();
            (seg[0] & 0xfe00) == 0xfc00 || (seg[0] & 0xffc0) == 0xfe80
        }
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
    fn test_load_empty_path_returns_none() {
        // Empty path short-circuits before touching the filesystem.
        assert!(GeoIpService::load("").is_none());
    }

    #[test]
    fn test_load_existing_but_invalid_file_returns_none() {
        use std::io::Write;

        // A real file that exists but is not a valid MMDB exercises the
        // `Reader::open_readfile` error branch (distinct from the
        // "missing path" short-circuit above).
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("not-a-real.mmdb");
        {
            let mut f = std::fs::File::create(&path).expect("create file");
            f.write_all(b"this is not a valid mmdb file at all")
                .expect("write garbage");
        }

        let path_str = path.to_str().expect("utf8 path");
        // File exists, so the existence check passes, but parsing fails -> None.
        assert!(GeoIpService::load(path_str).is_none());
    }

    #[test]
    fn test_load_from_bytes_empty_data() {
        // Empty byte slice is also invalid MMDB input.
        let result = GeoIpService::load_from_bytes(Vec::new(), "empty.mmdb".into());
        assert!(result.is_err());
    }

    #[test]
    fn test_load_from_bytes_error_message_format() {
        // The error message is wrapped with the documented prefix so callers
        // (and the status endpoint) get a consistent, human-readable string.
        let err = GeoIpService::load_from_bytes(vec![0xFF, 0xFF, 0xFF], "bad.mmdb".into())
            .err()
            .expect("invalid bytes must fail");
        assert!(
            err.starts_with("Invalid MMDB data:"),
            "unexpected error message: {err}"
        );
    }

    #[test]
    fn test_dbip_filename_constant() {
        // Guard the on-disk filename used by both download and status code.
        assert_eq!(DBIP_FILENAME, "dbip-country-lite.mmdb");
        assert!(DBIP_FILENAME.ends_with(".mmdb"));
    }

    #[test]
    fn test_geo_lookup_struct_fields() {
        // The public result struct is a plain data carrier; exercise both the
        // populated and the empty representations the lookup paths return.
        let populated = GeoLookup {
            country_code: Some("US".to_string()),
            region: Some("California".to_string()),
        };
        assert_eq!(populated.country_code.as_deref(), Some("US"));
        assert_eq!(populated.region.as_deref(), Some("California"));

        let empty = GeoLookup {
            country_code: None,
            region: None,
        };
        assert!(empty.country_code.is_none());
        assert!(empty.region.is_none());
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
        // Link-local and ULA are non-routable
        assert!(is_private(&"fe80::1".parse().unwrap()));
        assert!(is_private(&"fc00::1".parse().unwrap()));
        assert!(is_private(&"fd12:3456:789a::1".parse().unwrap()));

        // Loopback is handled separately by `is_loopback()` in `lookup`,
        // so it's not classified as "private" here.
        assert!(!is_private(&"::1".parse().unwrap()));

        // Global unicast
        assert!(!is_private(&"2001:db8::1".parse().unwrap()));
        assert!(!is_private(&"2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ipv4_172_boundaries() {
        // The 172.16.0.0/12 private block runs from 172.16.x to 172.31.x.
        // Addresses just outside the block must be classified as public.
        assert!(!is_private(&"172.15.255.255".parse().unwrap()));
        assert!(!is_private(&"172.32.0.0".parse().unwrap()));
        // And the very edges inside the block are private.
        assert!(is_private(&"172.16.0.0".parse().unwrap()));
        assert!(is_private(&"172.31.255.255".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ipv4_link_local_boundaries() {
        // 169.254.0.0/16 is link-local; 169.253.x and 169.255.x are not.
        assert!(is_private(&"169.254.0.0".parse().unwrap()));
        assert!(is_private(&"169.254.255.255".parse().unwrap()));
        assert!(!is_private(&"169.253.255.255".parse().unwrap()));
        assert!(!is_private(&"169.255.0.0".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ipv6_ula_boundaries() {
        // ULA is fc00::/7, i.e. fc00:: through fdff:: -> the first 7 bits match.
        // fbff:: is just below the range, fe00:: is just above (and is the
        // start of the fe80::/10 link-local check, which fe00:: does not match).
        assert!(!is_private(&"fbff::1".parse().unwrap()));
        assert!(is_private(&"fc00::1".parse().unwrap()));
        assert!(is_private(&"fdff:ffff::1".parse().unwrap()));
        // fe00:: is neither ULA (fc00::/7) nor link-local (fe80::/10).
        assert!(!is_private(&"fe00::1".parse().unwrap()));
    }

    #[test]
    fn test_is_private_ipv6_link_local_boundaries() {
        // Link-local is fe80::/10, i.e. fe80:: through febf::.
        assert!(is_private(&"fe80::1".parse().unwrap()));
        assert!(is_private(&"febf:ffff::1".parse().unwrap()));
        // fec0:: is outside the /10 link-local range -> public.
        assert!(!is_private(&"fec0::1".parse().unwrap()));
    }
}
