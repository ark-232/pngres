use axum::extract::State;
use axum::Json;

use crate::error::AppError;
use crate::index::Stats;
use crate::state::AppState;

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<Stats>, AppError> {
    Ok(Json(state.index.stats().await))
}
