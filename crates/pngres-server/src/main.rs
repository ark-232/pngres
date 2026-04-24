use std::path::PathBuf;

use pngres_server::{run, ServerConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cfg = ServerConfig {
        host: std::env::var("PNGRES_HOST").unwrap_or_else(|_| "127.0.0.1".to_owned()),
        port: std::env::var("PNGRES_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(6969),
        data_dir: PathBuf::from(
            std::env::var("PNGRES_DATA_DIR").unwrap_or_else(|_| "./herd".to_owned()),
        ),
        max_upload_bytes: std::env::var("PNGRES_MAX_UPLOAD_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2 * 1024 * 1024 * 1024),
    };

    run(cfg).await
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("pngres_server=info,tower_http=info"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
}
