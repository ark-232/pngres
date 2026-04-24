use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use humansize::{format_size, BINARY};
use pngres_core::{hash_bytes, hash_to_hex};

use super::CliError;
use crate::source::Source;

/// Decode a herd of monkes back into the original file.
#[derive(Args, Debug)]
pub struct DecodeArgs {
    /// A directory of monkes, or a .zip containing monkes.
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,

    /// Destination file (created; `-` writes to stdout).
    #[arg(long = "out", value_name = "FILE")]
    pub out: PathBuf,

    /// Skip whole-file BLAKE3 verification against the manifest.
    #[arg(long)]
    pub no_verify: bool,

    /// Overwrite destination file if it exists.
    #[arg(long)]
    pub force: bool,
}

pub fn run(args: DecodeArgs) -> Result<(), CliError> {
    let source = Source::detect(&args.source)?;
    eprintln!("Recalling monkes from {}...", source.path().display());

    let start = Instant::now();
    let decoded = source.load_all()?;
    let manifest = source.read_manifest()?;

    if decoded.len() as u32 != manifest.num_monkes {
        return Err(pngres_core::Error::MissingShard {
            index: decoded.len() as u32,
            total: manifest.num_monkes,
        }
        .into());
    }
    for (expected, s) in decoded.iter().enumerate() {
        if s.header.chunk_index as usize != expected {
            return Err(pngres_core::Error::MissingShard {
                index: expected as u32,
                total: manifest.num_monkes,
            }
            .into());
        }
    }

    if args.out.as_os_str() == "-" {
        let stdout = std::io::stdout();
        let mut lock = stdout.lock();
        let mut full = Vec::with_capacity(manifest.total_size as usize);
        for d in &decoded {
            full.extend_from_slice(&d.payload);
        }
        if !args.no_verify {
            verify(&full, &manifest)?;
        }
        lock.write_all(&full)?;
    } else {
        if args.out.exists() && !args.force {
            return Err(CliError::DestExists(args.out.clone()));
        }
        if let Some(parent) = args.out.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut out = std::fs::File::create(&args.out)?;
        let mut full = Vec::with_capacity(manifest.total_size as usize);
        for d in &decoded {
            full.extend_from_slice(&d.payload);
        }
        if !args.no_verify {
            verify(&full, &manifest)?;
        }
        out.write_all(&full)?;
        out.flush()?;
    }

    let elapsed = start.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        manifest.total_size as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };
    eprintln!(
        "Recalled {} monkes in {:.2?} ({}/s). size={} name={}",
        manifest.num_monkes,
        elapsed,
        format_size(rate as u64, BINARY),
        format_size(manifest.total_size, BINARY),
        manifest.filename
    );
    Ok(())
}

fn verify(bytes: &[u8], manifest: &pngres_core::Manifest) -> Result<(), CliError> {
    let got = hash_bytes(bytes);
    if got != manifest.file_hash {
        return Err(pngres_core::Error::HashMismatch {
            expected: hash_to_hex(&manifest.file_hash),
            got: hash_to_hex(&got),
        }
        .into());
    }
    Ok(())
}
