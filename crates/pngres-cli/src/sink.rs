//! Output sinks for encoded shards: directory or a single zip file.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use pngres_core::{shard_path, Manifest, Shard};
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use crate::commands::CliError;

pub trait ShardSink {
    fn write_shard(&mut self, shard: &Shard, total: u32) -> Result<(), CliError>;
    fn finish(&mut self, manifest: &Manifest) -> Result<(), CliError>;
}

pub struct DirSink {
    root: PathBuf,
}

impl DirSink {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn prepare(&self, force: bool) -> Result<(), CliError> {
        if self.root.exists() {
            if !force {
                let has_monkes = fs::read_dir(&self.root)?.any(|e| {
                    e.ok()
                        .and_then(|d| d.file_name().into_string().ok())
                        .map(|s| s.starts_with("monke_") && s.ends_with(".png"))
                        .unwrap_or(false)
                });
                if has_monkes {
                    return Err(CliError::DestExists(self.root.clone()));
                }
            }
        } else {
            fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl ShardSink for DirSink {
    fn write_shard(&mut self, shard: &Shard, total: u32) -> Result<(), CliError> {
        let p = shard_path(&self.root, shard.index, total);
        let mut f = File::create(&p)?;
        f.write_all(&shard.bytes)?;
        f.flush()?;
        Ok(())
    }

    fn finish(&mut self, manifest: &Manifest) -> Result<(), CliError> {
        let path = self.root.join("manifest.json");
        fs::write(&path, manifest.to_json_pretty()?)?;
        Ok(())
    }
}

pub struct ZipSink {
    writer: Option<ZipWriter<File>>,
    compression_level: i64,
}

impl ZipSink {
    pub fn new(path: &Path, force: bool, compression_level: u8) -> Result<Self, CliError> {
        if path.exists() && !force {
            return Err(CliError::DestExists(path.to_path_buf()));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(path)?;
        Ok(Self {
            writer: Some(ZipWriter::new(file)),
            compression_level: compression_level.min(9) as i64,
        })
    }

    fn file_name(index: u32, total: u32) -> String {
        let width = total.saturating_sub(1).to_string().len().max(3);
        format!("monke_{:0width$}.png", index + 1, width = width)
    }
}

impl ShardSink for ZipSink {
    fn write_shard(&mut self, shard: &Shard, total: u32) -> Result<(), CliError> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| CliError::Other("zip sink already finished".into()))?;
        let name = Self::file_name(shard.index, total);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(self.compression_level))
            .large_file(false);
        writer.start_file(name, opts)?;
        writer.write_all(&shard.bytes)?;
        Ok(())
    }

    fn finish(&mut self, manifest: &Manifest) -> Result<(), CliError> {
        let mut writer = self
            .writer
            .take()
            .ok_or_else(|| CliError::Other("zip sink already finished".into()))?;
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(self.compression_level));
        writer.start_file("manifest.json", opts)?;
        writer.write_all(manifest.to_json_pretty()?.as_bytes())?;
        writer.finish()?;
        Ok(())
    }
}
