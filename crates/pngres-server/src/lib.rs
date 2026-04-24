//! pngres-server — axum HTTP frontend to a local monke herd.
//!
//! No database. The filesystem is the database: `data/<file_id>/monke_*.png`
//! is authoritative, and every shard carries the full `Manifest` in its
//! `mkMf` ancillary chunk. We scan once at startup into an in-memory `Index`.

pub mod assets;
pub mod config;
pub mod error;
pub mod index;
pub mod routes;
pub mod state;
pub mod storage;

pub use config::ServerConfig;
pub use state::AppState;

pub async fn run(cfg: ServerConfig) -> anyhow::Result<()> {
    let state = AppState::init(&cfg).await?;
    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind((cfg.host.as_str(), cfg.port)).await?;
    tracing::info!("pngres-server listening on http://{}:{}", cfg.host, cfg.port);
    axum::serve(listener, app).await?;
    Ok(())
}
