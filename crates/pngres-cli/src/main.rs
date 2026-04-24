//! pngres — the CLI. Summon monkes. Recall monkes. Inspect monkes. Serve monkes.

use std::process::ExitCode;

use clap::Parser;

mod commands;
mod sink;
mod source;

#[derive(Parser)]
#[command(
    name = "pngres",
    version,
    about = "A really stupid but fully functional local DB. One monke = one unit of storage.",
    propagate_version = true
)]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: commands::Command,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match commands::dispatch(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("pngres: error: {err:#}");
            err.exit_code()
        }
    }
}

fn init_tracing(verbose: u8) {
    use tracing_subscriber::{fmt, EnvFilter};
    let level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}
