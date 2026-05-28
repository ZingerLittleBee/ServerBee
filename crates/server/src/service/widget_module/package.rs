use std::collections::HashMap;
use std::io::{Cursor, Read};

use super::WidgetModuleError;

/// In-memory representation of a module package, addressable by path.
pub struct UnpackedPackage {
    pub entries: HashMap<String, Vec<u8>>,
}

impl UnpackedPackage {
    /// A single-file package: entry_path is treated as the only file name.
    pub fn from_single_file(entry_path: &str, code: Vec<u8>) -> Self {
        let mut entries = HashMap::new();
        entries.insert(entry_path.to_string(), code);
        Self { entries }
    }

    /// Unpack a zip blob (defends against zip-slip, oversize entries, and
    /// zip-bomb style total uncompressed size or excessive entry counts).
    pub fn from_zip(blob: &[u8]) -> Result<Self, WidgetModuleError> {
        const MAX_ENTRY_BYTES: u64 = 5 * 1024 * 1024;
        const MAX_ZIP_ENTRIES: usize = 64;
        const MAX_TOTAL_UNCOMPRESSED: u64 = 32 * 1024 * 1024;

        let reader = Cursor::new(blob);
        let mut zip = zip::ZipArchive::new(reader)
            .map_err(|e| WidgetModuleError::ManifestExtraction(format!("invalid zip: {e}")))?;
        if zip.len() > MAX_ZIP_ENTRIES {
            return Err(WidgetModuleError::ManifestExtraction(
                "too many entries".into(),
            ));
        }
        let mut entries = HashMap::new();
        let mut total: u64 = 0;
        for i in 0..zip.len() {
            let mut entry = zip
                .by_index(i)
                .map_err(|e| WidgetModuleError::ManifestExtraction(format!("zip entry: {e}")))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .enclosed_name()
                .ok_or(WidgetModuleError::InvalidAssetPath)?
                .to_string_lossy()
                .to_string();
            if entry.size() > MAX_ENTRY_BYTES {
                return Err(WidgetModuleError::ManifestExtraction(format!(
                    "entry too large: {name}"
                )));
            }
            total = total.saturating_add(entry.size());
            if total > MAX_TOTAL_UNCOMPRESSED {
                return Err(WidgetModuleError::ManifestExtraction(
                    "zip too large".into(),
                ));
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut buf)
                .map_err(|e| WidgetModuleError::ManifestExtraction(format!("read: {e}")))?;
            entries.insert(name, buf);
        }
        Ok(Self { entries })
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        let normalised = path.trim_start_matches('/');
        if normalised.contains("..") {
            return None;
        }
        self.entries.get(normalised).map(|v| v.as_slice())
    }
}
