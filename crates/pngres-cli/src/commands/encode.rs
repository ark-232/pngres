use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use clap::Args;
use humansize::{format_size, BINARY};
use pngres_core::{encode_reader, CoverImage, EncodeOptions, LsbBits};

use super::CliError;
use crate::sink::{DirSink, ShardSink, ZipSink};

/// Encode a file into a herd of monkes.
#[derive(Args, Debug)]
#[command(group(
    clap::ArgGroup::new("output").required(true).args(["out_dir", "zip"]),
))]
pub struct EncodeArgs {
    /// Path to the file you are sacrificing to the monkes.
    #[arg(value_name = "INPUT")]
    pub input: PathBuf,

    /// Write numbered PNGs into this directory (created if missing).
    #[arg(long = "out", value_name = "DIR", group = "output")]
    pub out_dir: Option<PathBuf>,

    /// Write a single .zip of monkes to this path.
    #[arg(long = "zip", value_name = "ZIP", group = "output")]
    pub zip: Option<PathBuf>,

    /// LSB bits per channel to use for payload (1, 2, or 4).
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u8).range(1..=4))]
    pub bits: u8,

    /// Overwrite destination if it already contains monkes.
    #[arg(long)]
    pub force: bool,

    /// Deflate level for zip output (0..=9).
    #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u8).range(0..=9))]
    pub zip_level: u8,
}

pub fn run(args: EncodeArgs) -> Result<(), CliError> {
    let bits = match args.bits {
        1 => LsbBits::One,
        2 => LsbBits::Two,
        4 => LsbBits::Four,
        _ => return Err(CliError::Other(format!("unsupported --bits {}", args.bits))),
    };

    let cover = CoverImage::monke_default()?;
    let input_size = std::fs::metadata(&args.input)?.len();
    let filename = args
        .input
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_owned();

    eprintln!(
        "Summoning monkes for {} ({}) at {} bits/channel...",
        args.input.display(),
        format_size(input_size, BINARY),
        bits
    );

    let start = Instant::now();
    let opts = EncodeOptions {
        lsb_bits: bits,
        filename,
        ..Default::default()
    };
    let (manifest, shards) = {
        let f = std::fs::File::open(&args.input)?;
        encode_reader(BufReader::new(f), &cover, opts)?
    };

    let total = manifest.num_monkes;

    let mut sink: Box<dyn ShardSink> = if let Some(dir) = args.out_dir {
        let s = DirSink::new(dir);
        s.prepare(args.force)?;
        Box::new(s)
    } else {
        let z = ZipSink::new(args.zip.as_deref().unwrap(), args.force, args.zip_level)?;
        Box::new(z)
    };

    for shard in &shards {
        sink.write_shard(shard, total)?;
    }
    sink.finish(&manifest)?;

    let elapsed = start.elapsed();
    let rate = if elapsed.as_secs_f64() > 0.0 {
        (input_size as f64) / elapsed.as_secs_f64()
    } else {
        0.0
    };
    eprintln!(
        "Summoned {} monkes in {:.2?} ({}/s). file_id={}",
        total,
        elapsed,
        format_size(rate as u64, BINARY),
        manifest.file_id
    );
    Ok(())
}
