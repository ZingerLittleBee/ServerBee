pub mod error;
pub mod extractor;
pub mod loaded;
pub mod manifest;
pub mod service;

pub use error::SpaThemeError;
pub use loaded::LoadedTheme;
pub use manifest::ThemeManifest;
pub use service::SpaThemeService;

/// Maximum size (in bytes) of an uploaded `.sbtheme` package.
///
/// Re-exported by `router::api::spa_theme` for use in the multipart body
/// limit and 413 error payload. Centralized here so the error layer can
/// reference it without depending on the router module.
pub const UPLOAD_LIMIT_BYTES: u64 = 25 * 1024 * 1024;
