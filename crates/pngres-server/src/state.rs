use std::sync::Arc;

use pngres_core::CoverImage;

use crate::config::ServerConfig;
use crate::error::AppError;
use crate::events::{now_ts, Event, EventBus};
use crate::index::Index;
use crate::storage::FsStore;

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<Index>,
    pub cover: Arc<CoverImage>,
    pub events: EventBus,
    pub max_upload_bytes: usize,
}

impl AppState {
    pub async fn init(cfg: &ServerConfig) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(&cfg.data_dir).await?;
        let store = FsStore::new(cfg.data_dir.clone());
        let index = Arc::new(Index::new(store));
        index.rebuild().await?;

        let events = EventBus::new(512, 500);
        let (files, monkes) = {
            let s = index.stats().await;
            (s.total_files as usize, s.total_monkes as u64)
        };
        events.emit(Event::IndexReady {
            ts: now_ts(),
            files,
            monkes,
        });

        let cover = Arc::new(CoverImage::monke_default()?);
        Ok(Self {
            index,
            cover,
            events,
            max_upload_bytes: cfg.max_upload_bytes,
        })
    }
}
