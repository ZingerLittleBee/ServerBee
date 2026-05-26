use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::service::spa_theme::error::SpaThemeError;

pub const MAX_TOTAL_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_FILES: usize = 1000;
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_PREVIEW_BYTES: u64 = 500 * 1024;
pub const MAX_PATH_LEN: usize = 255;
pub const MAX_RATIO: u64 = 100;

pub const ALLOWED_EXTS: &[&str] = &[
    "html", "htm", "js", "mjs", "css", "png", "jpg", "jpeg", "svg", "webp", "gif", "ico", "woff", "woff2", "ttf",
    "otf", "json", "txt", "map",
];

#[derive(Debug)]
pub struct ExtractedPackage {
    pub files: HashMap<String, Vec<u8>>,
    pub manifest_bytes: Vec<u8>,
    pub preview: Option<(String, Vec<u8>, String /* mime */)>,
    pub total_bytes: u64,
}

pub fn extract(zip_bytes: &[u8]) -> Result<ExtractedPackage, SpaThemeError> {
    let cursor = std::io::Cursor::new(zip_bytes);
    let mut archive =
        zip::ZipArchive::new(cursor).map_err(|e| SpaThemeError::InvalidMultipart(format!("zip open: {e}")))?;

    if archive.len() > MAX_FILES {
        return Err(SpaThemeError::TooManyFiles { count: archive.len(), limit: MAX_FILES });
    }

    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut total: u64 = 0;
    let mut manifest_bytes: Option<Vec<u8>> = None;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| SpaThemeError::InvalidMultipart(format!("zip entry {i}: {e}")))?;

        if let Some(mode) = entry.unix_mode() {
            const S_IFMT: u32 = 0o170000;
            const S_IFLNK: u32 = 0o120000;
            if mode & S_IFMT == S_IFLNK {
                return Err(SpaThemeError::SymlinkNotAllowed { entry: entry.name().to_string() });
            }
        }
        if entry.is_dir() {
            continue;
        }

        let raw = entry.name().to_string();
        if raw.len() > MAX_PATH_LEN {
            return Err(SpaThemeError::ZipSlip { entry: raw });
        }
        let normalized = normalize_path(&raw).ok_or_else(|| SpaThemeError::ZipSlip { entry: raw.clone() })?;

        if !seen.insert(normalized.clone()) {
            return Err(SpaThemeError::DuplicateEntry { entry: normalized });
        }

        let ext = std::path::Path::new(&normalized)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        if !ALLOWED_EXTS.contains(&ext.as_str()) {
            return Err(SpaThemeError::DisallowedExtension { entry: normalized, ext });
        }

        let size = entry.size();
        if size > MAX_FILE_BYTES {
            return Err(SpaThemeError::FileTooLarge { entry: normalized, size, limit: MAX_FILE_BYTES });
        }

        let compressed = entry.compressed_size();
        if compressed > 0 {
            let ratio = size / compressed.max(1);
            if ratio > MAX_RATIO {
                return Err(SpaThemeError::ZipBomb { entry: normalized, ratio });
            }
        }

        total = total.saturating_add(size);
        if total > MAX_TOTAL_BYTES {
            return Err(SpaThemeError::TotalSizeExceeded { size: total, limit: MAX_TOTAL_BYTES });
        }

        // Defense-in-depth: the `size` above comes from the zip header and may lie.
        // Cap the actual read at MAX_FILE_BYTES + 1 so a crafted archive that
        // declares a small size but contains huge data cannot grow the buffer
        // unbounded. If the truncated read exceeds MAX_FILE_BYTES, reject.
        let mut buf = Vec::with_capacity(size.min(MAX_FILE_BYTES) as usize);
        let mut limited = (&mut entry).take(MAX_FILE_BYTES + 1);
        limited
            .read_to_end(&mut buf)
            .map_err(|e| SpaThemeError::InvalidMultipart(format!("read {normalized}: {e}")))?;
        if buf.len() as u64 > MAX_FILE_BYTES {
            return Err(SpaThemeError::FileTooLarge {
                entry: normalized,
                size: buf.len() as u64,
                limit: MAX_FILE_BYTES,
            });
        }

        if normalized == "manifest.json" {
            manifest_bytes = Some(buf.clone());
        }
        files.insert(normalized, buf);
    }

    let manifest_bytes = manifest_bytes.ok_or(SpaThemeError::MissingManifest)?;

    Ok(ExtractedPackage { files, manifest_bytes, preview: None, total_bytes: total })
}

fn normalize_path(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('/') {
        return None;
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' {
        return None;
    }
    if raw.contains('\\') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => continue,
            ".." => return None,
            other => parts.push(other),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join("/"))
}

pub fn locate_preview(
    files: &HashMap<String, Vec<u8>>,
    preview_path: &str,
) -> Result<Option<(String, Vec<u8>, String)>, SpaThemeError> {
    let Some(bytes) = files.get(preview_path) else {
        return Ok(None);
    };
    let size = bytes.len() as u64;
    if size > MAX_PREVIEW_BYTES {
        return Err(SpaThemeError::PreviewTooLarge { size, limit: MAX_PREVIEW_BYTES });
    }
    let mime = match preview_path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
    .to_string();
    Ok(Some((preview_path.to_string(), bytes.clone(), mime)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::SimpleFileOptions =
                zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf
    }

    #[test]
    fn extracts_minimal_package() {
        let zip = build_zip(&[
            ("manifest.json", br#"{"schema_version":1,"id":"a","name":"A","version":"1.0.0"}"#),
            ("index.html", b"<html></html>"),
        ]);
        let pkg = extract(&zip).unwrap();
        assert!(pkg.files.contains_key("index.html"));
        assert!(!pkg.manifest_bytes.is_empty());
    }

    #[test]
    fn rejects_zip_slip() {
        let zip = build_zip(&[("../etc/passwd", b"x"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::ZipSlip { .. }));
    }

    #[test]
    fn rejects_absolute_path() {
        let zip = build_zip(&[("/abs/path.html", b"x"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::ZipSlip { .. }));
    }

    #[test]
    fn rejects_disallowed_extension() {
        let zip = build_zip(&[("evil.sh", b"#!/bin/sh"), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::DisallowedExtension { .. }));
    }

    #[test]
    fn rejects_too_many_files() {
        let owned: Vec<(String, Vec<u8>)> = {
            let mut v: Vec<(String, Vec<u8>)> =
                (0..1001).map(|i| (format!("a{i}.txt"), vec![0u8; 8])).collect();
            v.push(("manifest.json".into(), b"{}".to_vec()));
            v
        };
        let refs: Vec<(&str, &[u8])> = owned.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        let zip = build_zip(&refs);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::TooManyFiles { .. }));
    }

    #[test]
    fn rejects_oversize_file() {
        let big = vec![b'a'; (MAX_FILE_BYTES + 1) as usize];
        let zip = build_zip(&[("big.js", &big), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::FileTooLarge { .. }));
    }

    #[test]
    fn rejects_total_over_limit() {
        // Use pseudo-random (incompressible) data to avoid triggering the ZipBomb check.
        // A simple LCG produces varied bytes that compress poorly.
        fn incompressible(seed: u64, len: usize) -> Vec<u8> {
            let mut v = Vec::with_capacity(len);
            let mut x = seed;
            for _ in 0..len {
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                v.push((x >> 56) as u8);
            }
            v
        }
        let owned: Vec<(String, Vec<u8>)> = {
            let mut v: Vec<(String, Vec<u8>)> = (0..5u64)
                .map(|i| (format!("c{i}.css"), incompressible(i + 1, MAX_FILE_BYTES as usize)))
                .collect();
            v.push(("manifest.json".into(), b"{}".to_vec()));
            v
        };
        let refs: Vec<(&str, &[u8])> = owned.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect();
        let zip = build_zip(&refs);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::TotalSizeExceeded { .. }));
    }

    #[test]
    fn rejects_missing_manifest() {
        let zip = build_zip(&[("index.html", b"<html/>")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::MissingManifest));
    }

    #[test]
    fn duplicate_entry_variant_exists_and_invalid_zip_handled() {
        // Coverage gap: the production `seen.insert` guard in extract() is not
        // exercised here because zip v2 prevents writing duplicate filenames and
        // silently deduplicates on read, so we cannot construct an archive that
        // makes by_index() yield the same name twice. The DuplicateEntry variant
        // remains in the extractor as defense-in-depth for future library swaps
        // or raw-zip inputs. This test verifies (a) the variant maps correctly
        // and (b) malformed zip bytes produce InvalidMultipart — the accepted
        // fallback in the original plan assertion.
        let buf = {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::<u8>::new()));
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("a.html", opts).unwrap();
            w.write_all(b"1").unwrap();
            w.start_file("manifest.json", opts).unwrap();
            w.write_all(b"{}").unwrap();
            w.finish().unwrap().into_inner()
        };
        let dup_err = SpaThemeError::DuplicateEntry { entry: "a.html".into() };
        assert_eq!(dup_err.code(), "DUPLICATE_ENTRY");
        let err = extract(b"PK not a real zip").unwrap_err();
        assert!(matches!(err, SpaThemeError::InvalidMultipart(_)));
        extract(&buf).expect("valid zip must succeed");
    }

    #[test]
    fn rejects_zip_bomb() {
        // Highly compressible: 5MB of zeros compresses to ~5KB → ratio ~1000x.
        let zeros = vec![0u8; MAX_FILE_BYTES as usize];
        let zip = build_zip(&[("bomb.js", &zeros), ("manifest.json", b"{}")]);
        let err = extract(&zip).unwrap_err();
        assert!(matches!(err, SpaThemeError::ZipBomb { .. }));
    }

    #[test]
    fn rejects_symlink() {
        // Build a zip with a real symlink entry via zip v2's add_symlink().
        // SimpleFileOptions::unix_permissions() masks with 0o777 and normalize()
        // forces S_IFREG for regular files, so the only way to produce an entry
        // with S_IFLNK set is the dedicated add_symlink() API.
        use zip::write::SimpleFileOptions;
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts = SimpleFileOptions::default();
            w.add_symlink("link.html", "target.html", opts).unwrap();
            w.start_file("manifest.json", opts).unwrap();
            std::io::Write::write_all(&mut w, b"{}").unwrap();
            w.finish().unwrap();
        }
        let err = extract(&buf).unwrap_err();
        assert!(matches!(err, SpaThemeError::SymlinkNotAllowed { .. }));
    }
}
