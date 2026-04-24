#![cfg(feature = "serve")]

use std::path::PathBuf;

use clap::Args;

use super::CliError;

/// Launch the pngres-server HTTP daemon.
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Host to bind to.
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,

    /// TCP port.
    #[arg(long, default_value_t = 6969)]
    pub port: u16,

    /// Directory that stores shard files on disk.
    #[arg(long, default_value = "./herd")]
    pub data_dir: PathBuf,

    /// Upload body cap in bytes (default: 2 GiB).
    #[arg(long)]
    pub max_upload_bytes: Option<usize>,
}

pub fn run(args: ServeArgs) -> Result<(), CliError> {
    let cfg = pngres_server::ServerConfig {
        host: args.host,
        port: args.port,
        data_dir: args.data_dir,
        max_upload_bytes: args.max_upload_bytes.unwrap_or(2 * 1024 * 1024 * 1024),
    };
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Other(format!("tokio: {e}")))?;
    rt.block_on(async move {
        pngres_server::run(cfg)
            .await
            .map_err(|e| CliError::Other(format!("server: {e}")))
    })
}
