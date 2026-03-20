use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Multipart, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::error::{ok, ApiResponse, AppError};
use crate::service::config::ConfigService;
use crate::state::AppState;

const CONFIG_KEY_BRAND: &str = "brand";
const MAX_IMAGE_SIZE: usize = 512 * 1024; // 512KB
const PNG_MAGIC: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];
const ICO_MAGIC: [u8; 4] = [0x00, 0x00, 0x01, 0x00];

#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct BrandConfig {
    pub logo_path: Option<String>,
    pub site_title: Option<String>,
    pub favicon_path: Option<String>,
    pub footer_text: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct UploadResponse {
    pub path: String,
}

/// Public routes (no auth required) — brand config + serving images.
pub fn public_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/brand", get(get_brand_config))
        .route("/brand/logo", get(serve_logo))
        .route("/brand/favicon", get(serve_favicon))
}

/// Admin-only routes — update config + upload images.
pub fn write_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/settings/brand", put(update_brand_config))
        .route("/settings/brand/logo", post(upload_logo))
        .route("/settings/brand/favicon", post(upload_favicon))
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[utoipa::path(
    get,
    path = "/api/settings/brand",
    tag = "brand",
    responses(
        (status = 200, description = "Brand config", body = BrandConfig),
    )
)]
pub async fn get_brand_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<BrandConfig>>, AppError> {
    let config: BrandConfig = ConfigService::get_typed(&state.db, CONFIG_KEY_BRAND)
        .await?
        .unwrap_or_default();
    ok(config)
}

#[utoipa::path(
    put,
    path = "/api/settings/brand",
    tag = "brand",
    request_body = BrandConfig,
    responses(
        (status = 200, description = "Brand config updated", body = BrandConfig),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn update_brand_config(
    State(state): State<Arc<AppState>>,
    Json(body): Json<BrandConfig>,
) -> Result<Json<ApiResponse<BrandConfig>>, AppError> {
    // Validate paths — must start with "/api/brand/" or be null
    if let Some(ref p) = body.logo_path
        && !p.starts_with("/api/brand/")
    {
        return Err(AppError::Validation(
            "logo_path must start with \"/api/brand/\" or be null".to_string(),
        ));
    }
    if let Some(ref p) = body.favicon_path
        && !p.starts_with("/api/brand/")
    {
        return Err(AppError::Validation(
            "favicon_path must start with \"/api/brand/\" or be null".to_string(),
        ));
    }

    ConfigService::set_typed(&state.db, CONFIG_KEY_BRAND, &body).await?;
    ok(body)
}

#[utoipa::path(
    post,
    path = "/api/settings/brand/logo",
    tag = "brand",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Logo uploaded", body = UploadResponse),
        (status = 400, description = "Invalid file"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn upload_logo(
    State(state): State<Arc<AppState>>,
    multipart: Multipart,
) -> Result<Json<ApiResponse<UploadResponse>>, AppError> {
    let (ext, data) = extract_and_validate_image(multipart).await?;
    let brand_dir = brand_dir(&state.config.server.data_dir);
    tokio::fs::create_dir_all(&brand_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create brand dir: {e}")))?;

    // Remove old logo files (any extension)
    remove_old_files(&brand_dir, "logo").await;

    let filename = format!("logo.{ext}");
    let path = brand_dir.join(&filename);
    tokio::fs::write(&path, &data)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write logo file: {e}")))?;

    // Update brand config with new logo path
    let mut config: BrandConfig = ConfigService::get_typed(&state.db, CONFIG_KEY_BRAND)
        .await?
        .unwrap_or_default();
    config.logo_path = Some("/api/brand/logo".to_string());
    ConfigService::set_typed(&state.db, CONFIG_KEY_BRAND, &config).await?;

    ok(UploadResponse {
        path: "/api/brand/logo".to_string(),
    })
}

#[utoipa::path(
    post,
    path = "/api/settings/brand/favicon",
    tag = "brand",
    request_body(content = String, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Favicon uploaded", body = UploadResponse),
        (status = 400, description = "Invalid file"),
        (status = 422, description = "Validation error"),
    ),
    security(("session_cookie" = []), ("api_key" = []))
)]
pub async fn upload_favicon(
    State(state): State<Arc<AppState>>,
    multipart: Multipart,
) -> Result<Json<ApiResponse<UploadResponse>>, AppError> {
    let (ext, data) = extract_and_validate_image(multipart).await?;
    let brand_dir = brand_dir(&state.config.server.data_dir);
    tokio::fs::create_dir_all(&brand_dir)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create brand dir: {e}")))?;

    // Remove old favicon files (any extension)
    remove_old_files(&brand_dir, "favicon").await;

    let filename = format!("favicon.{ext}");
    let path = brand_dir.join(&filename);
    tokio::fs::write(&path, &data)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write favicon file: {e}")))?;

    // Update brand config with new favicon path
    let mut config: BrandConfig = ConfigService::get_typed(&state.db, CONFIG_KEY_BRAND)
        .await?
        .unwrap_or_default();
    config.favicon_path = Some("/api/brand/favicon".to_string());
    ConfigService::set_typed(&state.db, CONFIG_KEY_BRAND, &config).await?;

    ok(UploadResponse {
        path: "/api/brand/favicon".to_string(),
    })
}

#[utoipa::path(
    get,
    path = "/api/brand/logo",
    tag = "brand",
    responses(
        (status = 200, description = "Logo image"),
        (status = 404, description = "No logo uploaded"),
    )
)]
pub async fn serve_logo(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    serve_brand_file(&state.config.server.data_dir, "logo").await
}

#[utoipa::path(
    get,
    path = "/api/brand/favicon",
    tag = "brand",
    responses(
        (status = 200, description = "Favicon image"),
        (status = 404, description = "No favicon uploaded"),
    )
)]
pub async fn serve_favicon(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    serve_brand_file(&state.config.server.data_dir, "favicon").await
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the brand assets directory path.
fn brand_dir(data_dir: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(data_dir).join("brand")
}

/// Extract the "file" field from multipart and validate size + magic bytes.
/// Returns `(extension, bytes)` where extension is "png" or "ico".
async fn extract_and_validate_image(
    mut multipart: Multipart,
) -> Result<(String, axum::body::Bytes), AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            let data = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {e}")))?;

            // Validate size
            if data.len() > MAX_IMAGE_SIZE {
                return Err(AppError::Validation(format!(
                    "File too large: {} bytes (max {})",
                    data.len(),
                    MAX_IMAGE_SIZE
                )));
            }
            if data.len() < 4 {
                return Err(AppError::Validation("File too small".to_string()));
            }

            // Validate magic bytes
            let magic: [u8; 4] = data[..4].try_into().unwrap();
            let ext = if magic == PNG_MAGIC {
                "png"
            } else if magic == ICO_MAGIC {
                "ico"
            } else {
                return Err(AppError::Validation(
                    "Invalid file type: only PNG and ICO are supported".to_string(),
                ));
            };

            return Ok((ext.to_string(), data));
        }
    }

    Err(AppError::BadRequest(
        "Missing 'file' field in multipart form".to_string(),
    ))
}

/// Remove old brand files matching a given prefix (e.g. "logo" removes logo.png, logo.ico).
async fn remove_old_files(dir: &std::path::Path, prefix: &str) {
    for ext in &["png", "ico"] {
        let path = dir.join(format!("{prefix}.{ext}"));
        let _ = tokio::fs::remove_file(&path).await;
    }
}

/// Find and serve a brand file (logo or favicon) from the brand directory.
async fn serve_brand_file(
    data_dir: &str,
    name: &str,
) -> Result<axum::response::Response, AppError> {
    let dir = brand_dir(data_dir);

    // Try each supported extension
    for (ext, content_type) in &[
        ("png", "image/png"),
        ("ico", "image/x-icon"),
    ] {
        let path = dir.join(format!("{name}.{ext}"));
        if path.exists() {
            let bytes = tokio::fs::read(&path)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to read {name} file: {e}")))?;
            return axum::response::Response::builder()
                .header(header::CONTENT_TYPE, *content_type)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(bytes))
                .map_err(|e| AppError::Internal(format!("Response build error: {e}")));
        }
    }

    Err(AppError::NotFound(format!("No {name} uploaded")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_png_magic_detection() {
        // Valid PNG header
        let png_data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let magic: [u8; 4] = png_data[..4].try_into().unwrap();
        assert_eq!(magic, PNG_MAGIC);
    }

    #[test]
    fn test_ico_magic_detection() {
        // Valid ICO header
        let ico_data = vec![0x00, 0x00, 0x01, 0x00, 0x01, 0x00];
        let magic: [u8; 4] = ico_data[..4].try_into().unwrap();
        assert_eq!(magic, ICO_MAGIC);
    }

    #[test]
    fn test_brand_config_default() {
        let config = BrandConfig::default();
        assert!(config.logo_path.is_none());
        assert!(config.site_title.is_none());
        assert!(config.favicon_path.is_none());
        assert!(config.footer_text.is_none());
    }

    #[test]
    fn test_brand_config_serialization() {
        let config = BrandConfig {
            logo_path: Some("/api/brand/logo".to_string()),
            site_title: Some("My Server".to_string()),
            favicon_path: None,
            footer_text: Some("Powered by ServerBee".to_string()),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: BrandConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.logo_path, config.logo_path);
        assert_eq!(parsed.site_title, config.site_title);
        assert_eq!(parsed.favicon_path, config.favicon_path);
        assert_eq!(parsed.footer_text, config.footer_text);
    }
}
