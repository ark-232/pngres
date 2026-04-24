use std::sync::Arc;

use pngres_core::CoverImage;

use crate::config::ServerConfig;
use crate::error::AppError;
use crate::index::Index;
use crate::storage::FsStore;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<Index>,
    pub cover: Arc<CoverImage>,
    pub max_upload_bytes: usize,
}

impl AppState {
    pub async fn init(cfg: &ServerConfig) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(&cfg.data_dir).await?;
        let store = FsStore::new(cfg.data_dir.clone());
        let index = Arc::new(Index::new(store));
        index.rebuild().await?;
        let cover = Arc::new(CoverImage::monke_default()?);
        Ok(Self {
            index,
            cover,
            max_upload_bytes: cfg.max_upload_bytes,
        })
    }
}
