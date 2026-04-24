use clap::Args;
use humansize::{format_size, BINARY};
use pngres_core::{payload_bytes_per_monke, raw_capacity_bytes, LsbBits};

use super::CliError;

/// Report capacity-per-monke at various LSB depths.
#[derive(Args, Debug)]
pub struct CapacityArgs {
    /// Bit depth to query (repeatable, e.g. --bits 1 --bits 2 --bits 4).
    /// If omitted, all three depths are reported.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=4))]
    pub bits: Vec<u8>,
}

pub fn run(args: CapacityArgs) -> Result<(), CliError> {
    let depths: Vec<LsbBits> = if args.bits.is_empty() {
        vec![LsbBits::One, LsbBits::Two, LsbBits::Four]
    } else {
        let mut v = Vec::new();
        for b in &args.bits {
            v.push(match b {
                1 => LsbBits::One,
                2 => LsbBits::Two,
                4 => LsbBits::Four,
                _ => return Err(CliError::Other(format!("unsupported --bits {b}"))),
            });
        }
        v
    };

    println!(
        "Cover: 650 x 800 RGBA, {} of LSB real estate total\n",
        format_size(650u64 * 800 * 4, BINARY)
    );
    println!(
        "{:<6} {:>14} {:>14} {:>12}",
        "bits", "raw capacity", "user payload", "monkes / GiB"
    );
    for b in depths {
        let raw = raw_capacity_bytes(b);
        let pay = payload_bytes_per_monke(b);
        let per_gib = (1024u64.pow(3)).div_ceil(pay as u64);
        println!(
            "{:<6} {:>14} {:>14} {:>12}",
            b,
            format_size(raw as u64, BINARY),
            format_size(pay as u64, BINARY),
            per_gib
        );
    }
    Ok(())
}
