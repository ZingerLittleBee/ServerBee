use std::io::Read;
use std::net::IpAddr;
use std::path::Path;

use chrono::Datelike;
use maxminddb::Reader;
use serde::Deserialize;

use crate::service::geoip::is_private;

/// Thread-safe ASN reader backed by MaxMind/DB-IP MMDB.
pub struct AsnService {
    reader: Reader<Vec<u8>>,
    /// Which file this was loaded from (for status endpoint).
    pub source_path: String,
}

#[derive(Deserialize)]
struct AsnRecord {
    /// DB-IP Lite ASN uses this field name.
    autonomous_system_number: Option<u32>,
    /// DB-IP Lite ASN organization name (kept aligned with MaxMind GeoLite2-ASN).
    autonomous_system_organization: Option<String>,
}

/// Default filename for the downloaded DB-IP Lite ASN database.
pub const DBIP_ASN_FILENAME: &str = "dbip-asn-lite.mmdb";

impl AsnService {
    /// Load from a file path. Returns None if file doesn't exist or is invalid.
    pub fn load(mmdb_path: &str) -> Option<Self> {
        if mmdb_path.is_empty() || !Path::new(mmdb_path).exists() {
            return None;
        }

        match Reader::open_readfile(mmdb_path) {
            Ok(reader) => {
                tracing::info!("ASN MMDB loaded from {mmdb_path}");
                Some(Self {
                    reader,
                    source_path: mmdb_path.to_string(),
                })
            }
            Err(e) => {
                tracing::error!("Failed to load ASN MMDB from {mmdb_path}: {e}");
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

    /// Lookup an IP address and return a display string like "AS15169 Google LLC".
    /// Returns None for private/loopback IPs or unknown IPs.
    pub fn lookup(&self, ip: IpAddr) -> Option<String> {
        if ip.is_loopback() || is_private(&ip) {
            return None;
        }

        let record = self.reader.lookup(ip).ok()?;
        let decoded: Option<AsnRecord> = record.decode().ok()?;
        let decoded = decoded?;
        match (decoded.autonomous_system_number, decoded.autonomous_system_organization) {
            (Some(num), Some(org)) => Some(format!("AS{num} {org}")),
            (Some(num), None) => Some(format!("AS{num}")),
            (None, Some(org)) => Some(org),
            (None, None) => None,
        }
    }
}

/// Download DB-IP Lite ASN MMDB, decompress, save to data_dir, return loaded service.
pub async fn download_dbip_asn(data_dir: &str) -> Result<AsnService, String> {
    let now = chrono::Utc::now();
    let url = format!(
        "https://download.db-ip.com/free/dbip-asn-lite-{}-{:02}.mmdb.gz",
        now.year(),
        now.month()
    );
    tracing::info!("Downloading ASN database from {url}");

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
    let final_path = Path::new(data_dir).join(DBIP_ASN_FILENAME);
    let service =
        AsnService::load_from_bytes(decompressed.clone(), final_path.display().to_string())?;

    // Atomic write: tmp file then rename
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed to create data directory: {e}"))?;
    let tmp_path = Path::new(data_dir).join(format!("{DBIP_ASN_FILENAME}.tmp"));
    std::fs::write(&tmp_path, &decompressed)
        .map_err(|e| format!("Failed to write database: {e}"))?;
    std::fs::rename(&tmp_path, &final_path).map_err(|e| format!("Failed to save database: {e}"))?;

    tracing::info!("ASN database saved to {}", final_path.display());
    Ok(service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_returns_none() {
        assert!(AsnService::load("").is_none());
        assert!(AsnService::load("/nonexistent/path.mmdb").is_none());
    }

    #[test]
    fn test_load_from_bytes_invalid_data() {
        let result = AsnService::load_from_bytes(vec![0, 1, 2, 3], "test.mmdb".into());
        assert!(result.is_err());
    }
}
