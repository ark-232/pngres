//! Filesystem-backed shard storage. Wrapping behind a thin surface so a future
//! S3/object_store impl can slot in without changing call sites.

use std::path::{Path, PathBuf};

use pngres_core::{shard_path, Shard};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::AppError;

#[derive(Clone)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn ensure_dir(&self, file_id: &str) -> Result<PathBuf, AppError> {
        let dir = self.root.join(file_id);
        fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    pub fn shard_path(&self, file_id: &str, index: u32, total: u32) -> PathBuf {
        shard_path(&self.root.join(file_id), index, total)
    }

    pub async fn write_shard(
        &self,
        file_id: &str,
        shard: &Shard,
        total: u32,
    ) -> Result<(), AppError> {
        let path = self.shard_path(file_id, shard.index, total);
        let mut f = fs::File::create(&path).await?;
        f.write_all(&shard.bytes).await?;
        f.flush().await?;
        Ok(())
    }

    pub async fn remove_dir(&self, file_id: &str) -> Result<(), AppError> {
        let dir = self.root.join(file_id);
        if dir.exists() {
            fs::remove_dir_all(&dir).await?;
        }
        Ok(())
    }
}
