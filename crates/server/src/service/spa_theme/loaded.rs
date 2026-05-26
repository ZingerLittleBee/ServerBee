use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use axum::body::Bytes;

use crate::service::spa_theme::manifest::ThemeManifest;

#[derive(Debug, Clone)]
pub struct LoadedTheme {
    pub uuid: String,
    pub manifest: ThemeManifest,
    pub entry: String,
    pub files: HashMap<String, Bytes>,
}

pub type ActiveSpaThemeSlot = Arc<ArcSwap<Option<LoadedTheme>>>;

pub fn new_slot() -> ActiveSpaThemeSlot {
    Arc::new(ArcSwap::from_pointee(None))
}

impl LoadedTheme {
    pub fn from_extracted(
        uuid: String,
        manifest: ThemeManifest,
        files: HashMap<String, Vec<u8>>,
    ) -> Self {
        let entry = manifest.entry.clone();
        let files = files.into_iter().map(|(k, v)| (k, Bytes::from(v))).collect();
        Self { uuid, manifest, entry, files }
    }

    pub fn get(&self, path: &str) -> Option<Bytes> {
        let p = if path.is_empty() || path == "/" { self.entry.as_str() } else { path.trim_start_matches('/') };
        self.files.get(p).cloned()
    }

    pub fn entry_html(&self) -> Option<Bytes> {
        self.files.get(&self.entry).cloned()
    }
}
