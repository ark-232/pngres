//! Input sources for decoding: directory or a single zip file.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use pngres_core::{decode_shard_bytes, DecodedShard};

use crate::commands::CliError;

pub enum Source {
    Dir(PathBuf),
    Zip(PathBuf),
}

impl Source {
    pub fn detect(path: &Path) -> Result<Self, CliError> {
        if path.is_dir() {
            Ok(Source::Dir(path.to_path_buf()))
        } else if path.extension().and_then(|s| s.to_str()) == Some("zip")
            || path
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| s.ends_with(".zip"))
                .unwrap_or(false)
        {
            Ok(Source::Zip(path.to_path_buf()))
        } else {
            Err(CliError::UnknownSource(path.to_path_buf()))
        }
    }

    /// Load and decode every shard. Returns in chunk_index order.
    pub fn load_all(&self) -> Result<Vec<DecodedShard>, CliError> {
        let mut raw: Vec<Vec<u8>> = Vec::new();
        match self {
            Source::Dir(p) => {
                for entry in fs::read_dir(p)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("monke_") && name_str.ends_with(".png") {
                        raw.push(fs::read(entry.path())?);
                    }
                }
            }
            Source::Zip(p) => {
                let file = fs::File::open(p)?;
                let mut zip = zip::ZipArchive::new(file)?;
                for i in 0..zip.len() {
                    let mut entry = zip.by_index(i)?;
                    if entry.is_file()
                        && entry.name().starts_with("monke_")
                        && entry.name().ends_with(".png")
                    {
                        let mut buf = Vec::with_capacity(entry.size() as usize);
                        entry.read_to_end(&mut buf)?;
                        raw.push(buf);
                    }
                }
            }
        }
        if raw.is_empty() {
            return Err(CliError::Other(format!(
                "no monkes found in {}",
                self.path().display()
            )));
        }
        let mut decoded: Vec<DecodedShard> =
            raw.into_iter().map(|b| decode_shard_bytes(&b)).collect::<Result<_, _>>()?;
        decoded.sort_by_key(|d| d.header.chunk_index);
        Ok(decoded)
    }

    /// Read just the manifest from any one shard (fast).
    pub fn read_manifest(&self) -> Result<pngres_core::Manifest, CliError> {
        match self {
            Source::Dir(p) => {
                for entry in fs::read_dir(p)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("monke_") && name_str.ends_with(".png") {
                        return Ok(pngres_core::read_manifest_from_shard(entry.path())?);
                    }
                }
                Err(CliError::Other(format!("no monkes found in {}", p.display())))
            }
            Source::Zip(p) => {
                let file = fs::File::open(p)?;
                let mut zip = zip::ZipArchive::new(file)?;
                for i in 0..zip.len() {
                    let mut entry = zip.by_index(i)?;
                    if entry.is_file()
                        && entry.name().starts_with("monke_")
                        && entry.name().ends_with(".png")
                    {
                        let mut buf = Vec::with_capacity(entry.size() as usize);
                        entry.read_to_end(&mut buf)?;
                        return Ok(pngres_core::read_manifest_from_shard_bytes(&buf)?);
                    }
                }
                Err(CliError::Other(format!("no monkes found in {}", p.display())))
            }
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Source::Dir(p) | Source::Zip(p) => p,
        }
    }
}
