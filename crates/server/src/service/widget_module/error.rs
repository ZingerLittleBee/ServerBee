use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum WidgetModuleError {
    #[error("manifest extraction failed: {0}")]
    ManifestExtraction(String),
    #[error("manifest validation failed: {0}")]
    ManifestValidation(String),
    #[error("module id conflict: {0}")]
    IdConflict(String),
    #[error("module not found: {0}")]
    NotFound(String),
    #[error("asset not found: {0}")]
    AssetNotFound(String),
    #[error("invalid asset path")]
    InvalidAssetPath,
    #[error("database: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<WidgetModuleError> for AppError {
    fn from(err: WidgetModuleError) -> Self {
        match err {
            WidgetModuleError::NotFound(msg) | WidgetModuleError::AssetNotFound(msg) => {
                AppError::NotFound(msg)
            }
            WidgetModuleError::IdConflict(msg) => AppError::Conflict(msg),
            WidgetModuleError::InvalidAssetPath
            | WidgetModuleError::ManifestExtraction(_)
            | WidgetModuleError::ManifestValidation(_) => AppError::BadRequest(err.to_string()),
            WidgetModuleError::Db(e) => AppError::Internal(format!("db error: {e}")),
        }
    }
}
