use std::path::PathBuf;

use clap::Args;
use humansize::{format_size, BINARY};
use pngres_core::{decode_shard_bytes, read_manifest_from_shard_bytes};
use serde::Serialize;

use super::CliError;

/// Print metadata baked into a single monke.
#[derive(Args, Debug)]
pub struct InspectArgs {
    /// Path to a monke.
    #[arg(value_name = "MONKE")]
    pub path: PathBuf,

    /// Emit JSON instead of a human-readable summary.
    #[arg(long)]
    pub json: bool,
}

#[derive(Serialize)]
struct InspectOutput {
    path: String,
    file_id: String,
    chunk_index: u32,
    total_chunks: u32,
    lsb_bits: u8,
    payload_length: u32,
    byte_offset: u64,
    total_file_size: u64,
    file_hash: String,
    filename: String,
    mime: String,
    num_monkes: u32,
}

pub fn run(args: InspectArgs) -> Result<(), CliError> {
    let bytes = std::fs::read(&args.path)?;
    let shard = decode_shard_bytes(&bytes)?;
    let manifest = read_manifest_from_shard_bytes(&bytes).ok();

    let file_hash = manifest
        .as_ref()
        .map(|m| pngres_core::hash_to_hex(&m.file_hash))
        .unwrap_or_default();
    let filename = manifest
        .as_ref()
        .map(|m| m.filename.clone())
        .unwrap_or_default();
    let mime = manifest
        .as_ref()
        .map(|m| m.mime.clone())
        .unwrap_or_default();
    let num_monkes = manifest
        .as_ref()
        .map(|m| m.num_monkes)
        .unwrap_or(shard.header.total_chunks);

    if args.json {
        let out = InspectOutput {
            path: args.path.display().to_string(),
            file_id: shard.header.file_id.to_string(),
            chunk_index: shard.header.chunk_index,
            total_chunks: shard.header.total_chunks,
            lsb_bits: shard.header.lsb_bits.as_u8(),
            payload_length: shard.header.payload_length,
            byte_offset: shard.header.byte_offset,
            total_file_size: shard.header.total_file_size,
            file_hash,
            filename,
            mime,
            num_monkes,
        };
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("Monke Report — {}", args.path.display());
        println!("  file_id        : {}", shard.header.file_id);
        println!(
            "  chunk          : {} of {}",
            shard.header.chunk_index + 1,
            shard.header.total_chunks
        );
        println!("  lsb bits       : {}", shard.header.lsb_bits);
        println!(
            "  payload        : {}  ({} B)",
            format_size(shard.header.payload_length as u64, BINARY),
            shard.header.payload_length
        );
        println!(
            "  byte offset    : {}  / total {}",
            format_size(shard.header.byte_offset, BINARY),
            format_size(shard.header.total_file_size, BINARY)
        );
        if let Some(m) = manifest {
            println!("  filename       : {}", m.filename);
            println!("  mime           : {}", m.mime);
            println!("  total monkes   : {}", m.num_monkes);
            println!("  file hash      : {}", pngres_core::hash_to_hex(&m.file_hash));
        } else {
            println!("  (mkMf chunk absent — this shard is orphaned from its manifest)");
        }
    }
    Ok(())
}
