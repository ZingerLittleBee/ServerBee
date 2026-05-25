use axum::http::StatusCode;
use serde_json::json;

use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum SpaThemeError {
    #[error("multipart upload exceeds the size limit")]
    UploadTooLarge { limit_bytes: u64 },
    #[error("multipart payload is malformed")]
    InvalidMultipart(String),
    #[error("manifest.json is missing")]
    MissingManifest,
    #[error("manifest is invalid")]
    InvalidManifest { field: &'static str, reason: String },
    #[error("entry HTML not present in package")]
    MissingEntry { entry: String },
    #[error("requires a newer ServerBee than running")]
    IncompatibleVersion { min: String, running: String },
    #[error("package contains unsafe path")]
    ZipSlip { entry: String },
    #[error("compression ratio too high")]
    ZipBomb { entry: String, ratio: u64 },
    #[error("symlinks are not allowed")]
    SymlinkNotAllowed { entry: String },
    #[error("duplicate zip entry")]
    DuplicateEntry { entry: String },
    #[error("file extension is not allowed")]
    DisallowedExtension { entry: String, ext: String },
    #[error("file too large")]
    FileTooLarge { entry: String, size: u64, limit: u64 },
    #[error("too many files in package")]
    TooManyFiles { count: usize, limit: usize },
    #[error("total uncompressed size exceeded")]
    TotalSizeExceeded { size: u64, limit: u64 },
    #[error("preview image too large")]
    PreviewTooLarge { size: u64, limit: u64 },
    #[error("version downgrade not allowed")]
    NoDowngrade { uploaded: String, existing: String },
    #[error("this version already exists")]
    VersionExists { manifest_id: String, version: String },
    #[error("theme is currently active")]
    ThemeInUse { uuid: String },
    #[error("theme not found")]
    ThemeNotFound { uuid: String },
}

impl SpaThemeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UploadTooLarge { .. } => "UPLOAD_TOO_LARGE",
            Self::InvalidMultipart(_) => "INVALID_MULTIPART",
            Self::MissingManifest => "MISSING_MANIFEST",
            Self::InvalidManifest { .. } => "INVALID_MANIFEST",
            Self::MissingEntry { .. } => "MISSING_ENTRY",
            Self::IncompatibleVersion { .. } => "INCOMPATIBLE_VERSION",
            Self::ZipSlip { .. } => "ZIP_SLIP",
            Self::ZipBomb { .. } => "ZIP_BOMB",
            Self::SymlinkNotAllowed { .. } => "SYMLINK_NOT_ALLOWED",
            Self::DuplicateEntry { .. } => "DUPLICATE_ENTRY",
            Self::DisallowedExtension { .. } => "DISALLOWED_EXTENSION",
            Self::FileTooLarge { .. } => "FILE_TOO_LARGE",
            Self::TooManyFiles { .. } => "TOO_MANY_FILES",
            Self::TotalSizeExceeded { .. } => "TOTAL_SIZE_EXCEEDED",
            Self::PreviewTooLarge { .. } => "PREVIEW_TOO_LARGE",
            Self::NoDowngrade { .. } => "NO_DOWNGRADE",
            Self::VersionExists { .. } => "VERSION_EXISTS",
            Self::ThemeInUse { .. } => "THEME_IN_USE",
            Self::ThemeNotFound { .. } => "THEME_NOT_FOUND",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::UploadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::VersionExists { .. } | Self::ThemeInUse { .. } => StatusCode::CONFLICT,
            Self::ThemeNotFound { .. } => StatusCode::NOT_FOUND,
            _ => StatusCode::BAD_REQUEST,
        }
    }

    fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::UploadTooLarge { limit_bytes } => Some(json!({ "limit_bytes": limit_bytes })),
            Self::InvalidMultipart(reason) => Some(json!({ "reason": reason })),
            Self::InvalidManifest { field, reason } => Some(json!({ "field": field, "reason": reason })),
            Self::MissingEntry { entry } => Some(json!({ "entry": entry })),
            Self::IncompatibleVersion { min, running } => Some(json!({ "min": min, "running": running })),
            Self::ZipSlip { entry } | Self::SymlinkNotAllowed { entry } | Self::DuplicateEntry { entry } => {
                Some(json!({ "entry": entry }))
            }
            Self::ZipBomb { entry, ratio } => Some(json!({ "entry": entry, "ratio": ratio })),
            Self::DisallowedExtension { entry, ext } => Some(json!({ "entry": entry, "ext": ext })),
            Self::FileTooLarge { entry, size, limit } => Some(json!({ "entry": entry, "size": size, "limit": limit })),
            Self::TooManyFiles { count, limit } => Some(json!({ "count": count, "limit": limit })),
            Self::TotalSizeExceeded { size, limit } | Self::PreviewTooLarge { size, limit } => {
                Some(json!({ "size": size, "limit": limit }))
            }
            Self::NoDowngrade { uploaded, existing } => Some(json!({ "uploaded": uploaded, "existing": existing })),
            Self::VersionExists { manifest_id, version } => {
                Some(json!({ "manifest_id": manifest_id, "version": version }))
            }
            Self::ThemeNotFound { uuid } | Self::ThemeInUse { uuid } => Some(json!({ "uuid": uuid })),
            Self::MissingManifest => None,
        }
    }
}

impl From<SpaThemeError> for AppError {
    fn from(err: SpaThemeError) -> Self {
        let status = err.status();
        let code = err.code();
        let details = err.details();
        let message = err.to_string();
        AppError::Domain { status, code, message, details }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;

    #[tokio::test]
    async fn zip_slip_maps_to_domain_error() {
        let err: AppError = SpaThemeError::ZipSlip { entry: "../etc/passwd".into() }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "ZIP_SLIP");
        assert_eq!(json["error"]["details"]["entry"], "../etc/passwd");
    }

    #[tokio::test]
    async fn theme_in_use_is_409() {
        let err: AppError = SpaThemeError::ThemeInUse { uuid: "abc".into() }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn upload_too_large_is_413() {
        let err: AppError = SpaThemeError::UploadTooLarge { limit_bytes: 25 * 1024 * 1024 }.into();
        let resp = err.into_response();
        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
