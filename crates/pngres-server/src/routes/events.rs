//! Server-Sent Events (SSE) live stream + snapshot endpoints.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::Json;
use futures::stream::{self, Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;

use crate::events::{Event, PeerSummary};
use crate::state::AppState;

/// SSE: live feed of server events. Sends a snapshot of recent events first
/// (so a fresh page-load is populated), then streams live events.
pub async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let snapshot = state.events.snapshot();
    let live = BroadcastStream::new(state.events.subscribe()).filter_map(|r| async move {
        r.ok().and_then(|ev| SseEvent::default().json_data(ev).ok())
    });

    let initial = stream::iter(
        snapshot
            .into_iter()
            .filter_map(|ev| SseEvent::default().json_data(ev).ok()),
    );

    let merged = initial.chain(live).map(Ok::<_, Infallible>);

    Sse::new(merged).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// JSON snapshot of recent events (polling fallback if SSE is blocked).
pub async fn activity_snapshot(State(state): State<AppState>) -> Json<Vec<Event>> {
    Json(state.events.snapshot())
}

/// JSON list of peer IPs seen in the ring buffer.
pub async fn peers(State(state): State<AppState>) -> Json<Vec<PeerSummary>> {
    Json(state.events.peers())
}
