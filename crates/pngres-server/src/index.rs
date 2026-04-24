//! Filesystem-backed index. No external database.
//!
//! The source of truth is `data/<file_id>/monke_*.png`. Each shard carries
//! the full file-level `Manifest` as JSON inside its `mkMf` ancillary chunk.
//! We scan that once at startup to build an in-memory cache, then update the
//! cache on writes and deletes. The disk never lies.

use std::collections::HashMap;
use std::path::PathBuf;

use pngres_core::{read_manifest_from_shard, Manifest};
use serde::Serialize;
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::AppError;
use crate::storage::FsStore;

#[derive(Clone, Debug)]
pub struct IndexEntry {
    pub manifest: Manifest,
}

impl IndexEntry {
    pub fn summary(&self) -> FileSummary {
        FileSummary {
            id: self.manifest.file_id.to_string(),
            name: self.manifest.filename.clone(),
            mime: self.manifest.mime.clone(),
            size: self.manifest.total_size as i64,
            num_monkes: self.manifest.num_monkes as i64,
            lsb_bits: self.manifest.lsb_bits.as_u8() as i64,
            per_monke_payload_bytes: self.manifest.payload_per_monke as i64,
            created_at: self.manifest.created_at as i64,
            blake3: pngres_core::hash_to_hex(&self.manifest.file_hash),
        }
    }
}

/// Wire shape identical to the old `db::FileRow` so API consumers don't
/// notice the storage swap.
#[derive(Clone, Debug, Serialize)]
pub struct FileSummary {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub num_monkes: i64,
    pub lsb_bits: i64,
    pub per_monke_payload_bytes: i64,
    pub created_at: i64,
    pub blake3: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Stats {
    pub total_files: i64,
    pub total_bytes: i64,
    pub total_monkes: i64,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
}

pub struct Index {
    store: FsStore,
    entries: RwLock<HashMap<Uuid, IndexEntry>>,
}

impl Index {
    pub fn new(store: FsStore) -> Self {
        Self {
            store,
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn store(&self) -> &FsStore {
        &self.store
    }

    /// Walk `data/` once. Each subdirectory whose name parses as a UUID is a
    /// file; read the first `monke_*.png` in it and pull the manifest.
    pub async fn rebuild(&self) -> Result<(), AppError> {
        let root = self.store.root().to_path_buf();
        fs::create_dir_all(&root).await?;

        let loaded = tokio::task::spawn_blocking(move || -> Result<Vec<(Uuid, Manifest)>, AppError> {
            let mut out = Vec::new();
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let Ok(id) = Uuid::parse_str(&name_str) else {
                    continue;
                };
                let Some(first_shard) = first_monke_in_dir(&entry.path())? else {
                    continue;
                };
                match read_manifest_from_shard(&first_shard) {
                    Ok(m) => out.push((id, m)),
                    Err(e) => {
                        tracing::warn!(dir=%entry.path().display(), error=%e, "skipping corrupt herd");
                    }
                }
            }
            Ok(out)
        })
        .await
        .map_err(|e| AppError::Other(format!("rebuild join: {e}")))??;

        let mut guard = self.entries.write().await;
        guard.clear();
        for (id, manifest) in loaded {
            guard.insert(id, IndexEntry { manifest });
        }
        let n = guard.len();
        tracing::info!(files = n, "index ready");
        Ok(())
    }

    pub async fn get(&self, id: &Uuid) -> Option<IndexEntry> {
        self.entries.read().await.get(id).cloned()
    }

    pub async fn get_by_str(&self, id: &str) -> Option<IndexEntry> {
        let parsed = Uuid::parse_str(id).ok()?;
        self.get(&parsed).await
    }

    /// List entries, newest-first, with limit/offset pagination.
    pub async fn list(&self, limit: i64, offset: i64) -> Vec<IndexEntry> {
        let guard = self.entries.read().await;
        let mut all: Vec<IndexEntry> = guard.values().cloned().collect();
        all.sort_by(|a, b| {
            b.manifest
                .created_at
                .cmp(&a.manifest.created_at)
                .then_with(|| b.manifest.file_id.cmp(&a.manifest.file_id))
        });
        all.into_iter()
            .skip(offset.max(0) as usize)
            .take(limit.max(0) as usize)
            .collect()
    }

    pub async fn insert(&self, manifest: Manifest) {
        self.entries
            .write()
            .await
            .insert(manifest.file_id, IndexEntry { manifest });
    }

    /// Remove from cache + disk.
    pub async fn remove(&self, id: &Uuid) -> Result<bool, AppError> {
        let had = self.entries.write().await.remove(id).is_some();
        self.store.remove_dir(&id.to_string()).await?;
        Ok(had)
    }

    pub async fn stats(&self) -> Stats {
        let guard = self.entries.read().await;
        if guard.is_empty() {
            return Stats {
                total_files: 0,
                total_bytes: 0,
                total_monkes: 0,
                oldest: None,
                newest: None,
            };
        }
        let mut total_files = 0i64;
        let mut total_bytes = 0i64;
        let mut total_monkes = 0i64;
        let mut oldest: Option<i64> = None;
        let mut newest: Option<i64> = None;
        for e in guard.values() {
            total_files += 1;
            total_bytes += e.manifest.total_size as i64;
            total_monkes += e.manifest.num_monkes as i64;
            let t = e.manifest.created_at as i64;
            oldest = Some(oldest.map_or(t, |o| o.min(t)));
            newest = Some(newest.map_or(t, |n| n.max(t)));
        }
        Stats {
            total_files,
            total_bytes,
            total_monkes,
            oldest,
            newest,
        }
    }
}

fn first_monke_in_dir(dir: &std::path::Path) -> std::io::Result<Option<PathBuf>> {
    let mut names: Vec<PathBuf> = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let n = e.file_name();
        let s = n.to_string_lossy();
        if s.starts_with("monke_") && s.ends_with(".png") {
            names.push(e.path());
        }
    }
    names.sort();
    Ok(names.into_iter().next())
}
