use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

mod files;
mod pages;
mod stats;
mod stream;
mod upload;

pub fn router(state: AppState) -> Router {
    let max = state.max_upload_bytes;

    // Upload route has its own (very large) body limit so multipart doesn't hit
    // axum's default 2 MiB cap. Other routes stay bounded.
    let upload_router = Router::new()
        .route("/upload", post(upload::upload))
        .layer(DefaultBodyLimit::max(max));

    let api = Router::new()
        .route("/stats", get(stats::get_stats))
        .route("/files", get(files::list))
        .route("/files/:id", get(files::get).delete(files::delete_file))
        .route("/files/:id/download", get(files::download))
        .route("/files/:id/stream", get(stream::stream))
        .route("/files/:id/monkes", get(files::list_monkes))
        .route("/files/:id/monkes/:n", get(files::get_monke))
        .merge(upload_router);

    Router::new()
        .nest("/api", api)
        .route("/", get(pages::index))
        .route("/static/*path", get(pages::static_asset))
        .route("/healthz", get(|| async { "ok" }))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}
