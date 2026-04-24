//! `pngres remote` — point at another PNGres instance and pull monkes.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use clap::{Args, Subcommand};
use humansize::{format_size, BINARY};
use pngres_core::{decode_shard_bytes, hash_bytes, hash_to_hex};
use reqwest::blocking::Client;
use reqwest::header;
use serde::{Deserialize, Serialize};

use super::CliError;

/// Connect to a remote PNGres server and pull monkes off of it.
#[derive(Args, Debug)]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub action: RemoteAction,
}

#[derive(Subcommand, Debug)]
pub enum RemoteAction {
    /// List files on a remote PNGres.
    List(ListArgs),
    /// Pull and decode a file from a remote into a local file (uses /stream).
    Pull(PullArgs),
    /// Pull the raw monke PNG shards from a remote into a local directory.
    PullMonkes(PullMonkesArgs),
    /// Inspect a remote file's monke index without downloading anything.
    Discover(DiscoverArgs),
    /// Pull every file from a remote into a local directory (one subdir per file).
    Mirror(MirrorArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Base URL of the remote PNGres, e.g. http://192.168.1.10:6969
    #[arg(value_name = "URL")]
    pub url: String,
    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct PullArgs {
    /// Base URL of the remote PNGres.
    #[arg(value_name = "URL")]
    pub url: String,
    /// The file_id to pull.
    #[arg(value_name = "FILE_ID")]
    pub id: String,
    /// Local output path.
    #[arg(long, value_name = "FILE")]
    pub out: PathBuf,
    /// Overwrite if it exists.
    #[arg(long)]
    pub force: bool,
    /// Skip BLAKE3 verification.
    #[arg(long)]
    pub no_verify: bool,
}

#[derive(Args, Debug)]
pub struct PullMonkesArgs {
    #[arg(value_name = "URL")]
    pub url: String,
    #[arg(value_name = "FILE_ID")]
    pub id: String,
    #[arg(long, value_name = "DIR")]
    pub out: PathBuf,
    #[arg(long)]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct DiscoverArgs {
    #[arg(value_name = "URL")]
    pub url: String,
    #[arg(value_name = "FILE_ID")]
    pub id: String,
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct MirrorArgs {
    #[arg(value_name = "URL")]
    pub url: String,
    /// Root directory. Each file lands at `<OUT>/<file_id>/<original_name>`.
    #[arg(long, value_name = "DIR")]
    pub out: PathBuf,
    #[arg(long)]
    pub force: bool,
}

// ------------------------------------------------------------------ helpers

fn client() -> Result<Client, CliError> {
    Client::builder()
        .user_agent(format!("pngres-cli/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| CliError::Other(format!("reqwest: {e}")))
}

fn join(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

fn http_error<R: Into<String>>(action: &str, status: u16, resp_text: R) -> CliError {
    CliError::Other(format!(
        "{action} failed: HTTP {} — {}",
        status,
        truncate(resp_text.into(), 200)
    ))
}

fn truncate(mut s: String, n: usize) -> String {
    if s.len() > n {
        s.truncate(n);
        s.push_str("…");
    }
    s
}

// ------------------------------------------------------------------ shapes (mirror the server's JSON)

#[derive(Debug, Deserialize, Serialize)]
struct FileSummary {
    id: String,
    name: String,
    mime: String,
    size: i64,
    num_monkes: i64,
    lsb_bits: i64,
    created_at: i64,
    blake3: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    items: Vec<FileSummary>,
    #[allow(dead_code)]
    total: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct MonkeRef {
    index: u32,
    url: String,
    filename: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct MonkeIndex {
    id: String,
    name: String,
    mime: String,
    size: i64,
    num_monkes: u32,
    lsb_bits: i64,
    per_monke_payload_bytes: i64,
    file_hash: String,
    monkes: Vec<MonkeRef>,
}

// ------------------------------------------------------------------ dispatch

pub fn run(args: RemoteArgs) -> Result<(), CliError> {
    match args.action {
        RemoteAction::List(a) => list(a),
        RemoteAction::Pull(a) => pull(a),
        RemoteAction::PullMonkes(a) => pull_monkes(a),
        RemoteAction::Discover(a) => discover(a),
        RemoteAction::Mirror(a) => mirror(a),
    }
}

fn list(args: ListArgs) -> Result<(), CliError> {
    let c = client()?;
    let resp = c
        .get(join(&args.url, "/api/files"))
        .send()
        .map_err(|e| CliError::Other(format!("GET /api/files: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let body = resp.text().unwrap_or_default();
        return Err(http_error("remote list", s, body));
    }
    let data: ListResponse = resp
        .json()
        .map_err(|e| CliError::Other(format!("list json: {e}")))?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&data.items)?);
        return Ok(());
    }
    if data.items.is_empty() {
        println!("(the remote herd is empty)");
        return Ok(());
    }
    println!(
        "{:<38} {:>6} {:>12}  {:<18}  {}",
        "file_id", "monkes", "size", "mime", "name"
    );
    for it in &data.items {
        println!(
            "{:<38} {:>6} {:>12}  {:<18}  {}",
            it.id,
            it.num_monkes,
            format_size(it.size.max(0) as u64, BINARY),
            truncate(it.mime.clone(), 18),
            it.name
        );
    }
    Ok(())
}

fn pull(args: PullArgs) -> Result<(), CliError> {
    if args.out.exists() && !args.force {
        return Err(CliError::DestExists(args.out.clone()));
    }
    if let Some(p) = args.out.parent() {
        if !p.as_os_str().is_empty() {
            fs::create_dir_all(p)?;
        }
    }

    let c = client()?;
    let url = join(&args.url, &format!("/api/files/{}/stream", args.id));
    eprintln!("Pulling {} -> {}", url, args.out.display());

    let start = Instant::now();
    let mut resp = c
        .get(&url)
        .send()
        .map_err(|e| CliError::Other(format!("GET stream: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status().as_u16();
        let body = resp.text().unwrap_or_default();
        return Err(http_error("remote pull", s, body));
    }
    let len_hint = resp
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());

    let mut out = fs::File::create(&args.out)?;
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    let mut hasher = blake3::Hasher::new();
    loop {
        let n = resp
            .read(&mut buf)
            .map_err(|e| CliError::Other(format!("read: {e}")))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    out.flush()?;

    // Verify via the dedicated metadata endpoint (source of truth for file_hash).
    if !args.no_verify {
        let meta_url = join(&args.url, &format!("/api/files/{}", args.id));
        let meta: serde_json::Value = c
            .get(meta_url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| CliError::Other(format!("verify fetch: {e}")))?;
        if let Some(want_hex) =
            meta.pointer("/manifest/file_hash").and_then(|v| v.as_array())
        {
            let bytes: Vec<u8> = want_hex
                .iter()
                .filter_map(|v| v.as_u64().map(|n| n as u8))
                .collect();
            if bytes.len() == 32 {
                let got: [u8; 32] = *hasher.finalize().as_bytes();
                let want: [u8; 32] = bytes.try_into().unwrap();
                if got != want {
                    fs::remove_file(&args.out).ok();
                    return Err(pngres_core::Error::HashMismatch {
                        expected: hash_to_hex(&want),
                        got: hash_to_hex(&got),
                    }
                    .into());
                }
            }
        }
    }

    let elapsed = start.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        (total as f64) / elapsed.as_secs_f64()
    } else {
        0.0
    };
    eprintln!(
        "Pulled {} in {:.2?} ({}/s){}",
        format_size(total, BINARY),
        elapsed,
        format_size(rate as u64, BINARY),
        if let Some(l) = len_hint {
            if l == total {
                ""
            } else {
                " (size hint mismatch)"
            }
        } else {
            ""
        }
    );
    Ok(())
}

fn pull_monkes(args: PullMonkesArgs) -> Result<(), CliError> {
    let c = client()?;
    let idx_url = join(&args.url, &format!("/api/files/{}/monkes", args.id));
    eprintln!("Fetching monke index: {}", idx_url);
    let idx: MonkeIndex = c
        .get(&idx_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| CliError::Other(format!("GET monkes: {e}")))?;

    if args.out.exists() {
        let has_monkes = fs::read_dir(&args.out)?.any(|e| {
            e.ok()
                .and_then(|d| d.file_name().into_string().ok())
                .map(|s| s.starts_with("monke_") && s.ends_with(".png"))
                .unwrap_or(false)
        });
        if has_monkes && !args.force {
            return Err(CliError::DestExists(args.out.clone()));
        }
    } else {
        fs::create_dir_all(&args.out)?;
    }

    let start = Instant::now();
    let mut total_bytes: u64 = 0;
    for m in &idx.monkes {
        let shard_url = join(&args.url, &m.url);
        let resp = c
            .get(&shard_url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| CliError::Other(format!("GET {}: {e}", shard_url)))?;
        let bytes = resp
            .bytes()
            .map_err(|e| CliError::Other(format!("read body: {e}")))?;
        let path = args.out.join(&m.filename);
        fs::write(&path, &bytes)?;

        // Verify each shard decodes and the embedded file_id matches.
        let shard = decode_shard_bytes(&bytes)?;
        if shard.header.file_id.to_string() != idx.id {
            return Err(CliError::Other(format!(
                "shard {} file_id {} != expected {}",
                m.index, shard.header.file_id, idx.id
            )));
        }
        if shard.header.chunk_index != m.index {
            return Err(CliError::Other(format!(
                "shard {} header says chunk_index={}",
                m.index, shard.header.chunk_index
            )));
        }
        total_bytes += bytes.len() as u64;
        eprintln!(
            "  pulled {} ({} B payload, {} on-the-wire)",
            m.filename,
            shard.header.payload_length,
            format_size(bytes.len() as u64, BINARY)
        );
    }

    let elapsed = start.elapsed();
    eprintln!(
        "Pulled {} monkes ({}) in {:.2?}. file_hash={}",
        idx.num_monkes,
        format_size(total_bytes, BINARY),
        elapsed,
        idx.file_hash
    );
    Ok(())
}

fn discover(args: DiscoverArgs) -> Result<(), CliError> {
    let c = client()?;
    let idx_url = join(&args.url, &format!("/api/files/{}/monkes", args.id));
    let idx: MonkeIndex = c
        .get(&idx_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| CliError::Other(format!("GET monkes: {e}")))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&idx).unwrap_or_default());
        return Ok(());
    }

    println!("Remote Monke Discovery — {}", args.url);
    println!("  file_id       : {}", idx.id);
    println!("  filename      : {}", idx.name);
    println!("  mime          : {}", idx.mime);
    println!(
        "  size          : {} ({} B)",
        format_size(idx.size as u64, BINARY),
        idx.size
    );
    println!("  num_monkes    : {}", idx.num_monkes);
    println!("  lsb_bits      : {}", idx.lsb_bits);
    println!(
        "  per_monke     : {}",
        format_size(idx.per_monke_payload_bytes as u64, BINARY)
    );
    println!("  file_hash     : {}", idx.file_hash);
    println!("  first monkes  : {}",
        idx.monkes
            .iter()
            .take(3)
            .map(|m| m.filename.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn mirror(args: MirrorArgs) -> Result<(), CliError> {
    let c = client()?;
    let list_url = join(&args.url, "/api/files");
    let resp: ListResponse = c
        .get(&list_url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|e| CliError::Other(format!("GET files: {e}")))?;

    if resp.items.is_empty() {
        eprintln!("(remote herd is empty — nothing to mirror)");
        return Ok(());
    }
    fs::create_dir_all(&args.out)?;

    let mut ok = 0usize;
    for it in &resp.items {
        let dir = args.out.join(&it.id);
        fs::create_dir_all(&dir)?;
        let out_file = dir.join(&it.name);
        if out_file.exists() && !args.force {
            eprintln!("skip {} ({} already exists)", it.name, out_file.display());
            continue;
        }
        let sub = PullArgs {
            url: args.url.clone(),
            id: it.id.clone(),
            out: out_file,
            force: args.force,
            no_verify: false,
        };
        match pull(sub) {
            Ok(()) => ok += 1,
            Err(e) => eprintln!("  failed {}: {e}", it.id),
        }
    }
    eprintln!("Mirrored {}/{} files.", ok, resp.items.len());
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn _verify_bytes(bytes: &[u8], want: &str) -> bool {
    let got = hash_to_hex(&hash_bytes(bytes));
    got == want
}
