//! Range-aware streaming endpoint. Feeds <video> tags.

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use futures::stream::{self, StreamExt};
use http_range_header::parse_range_header;
use pngres_core::{read_range, Manifest};

use crate::error::AppError;
use crate::events::{now_ts, Event};
use crate::state::AppState;

const OPEN_ENDED_CAP: u64 = 8 * 1024 * 1024;

pub async fn stream(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let entry = state
        .index
        .get_by_str(&id)
        .await
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let manifest = entry.manifest;
    let total = manifest.total_size;
    let name = manifest.filename.clone();
    let peer = addr.ip().to_string();

    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    if range_header.is_none() {
        let resp = full_response(state.clone(), &id, &manifest).await?;
        state.events.emit(Event::Stream {
            ts: now_ts(),
            peer: Some(peer),
            id,
            name,
            range_start: 0,
            range_end_inclusive: total.saturating_sub(1),
            slice_len: total,
        });
        return Ok(resp);
    }

    let parsed = parse_range_header(range_header.unwrap())
        .map_err(|e| AppError::InvalidRange(e.to_string()))?;
    let ranges = parsed
        .validate(total)
        .map_err(|e| AppError::InvalidRange(format!("{:?}", e)))?;
    if ranges.len() != 1 {
        return Err(AppError::InvalidRange("multi-range not supported".into()));
    }
    let r = &ranges[0];
    let req_start = *r.start();
    let req_end_inclusive = *r.end();

    let end_inclusive = if req_end_inclusive - req_start + 1 > OPEN_ENDED_CAP {
        (req_start + OPEN_ENDED_CAP - 1).min(total.saturating_sub(1))
    } else {
        req_end_inclusive
    };
    let slice_len = end_inclusive - req_start + 1;

    let body = stream_range(state.clone(), id.clone(), manifest.clone(), req_start..end_inclusive + 1);

    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, mime_header(&manifest.mime));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(
        header::CONTENT_RANGE,
        HeaderValue::from_str(&format!("bytes {}-{}/{}", req_start, end_inclusive, total))
            .unwrap(),
    );
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(slice_len));

    state.events.emit(Event::Stream {
        ts: now_ts(),
        peer: Some(peer),
        id,
        name,
        range_start: req_start,
        range_end_inclusive: end_inclusive,
        slice_len,
    });

    Ok((StatusCode::PARTIAL_CONTENT, h, Body::from_stream(body)).into_response())
}

async fn full_response(
    state: AppState,
    id: &str,
    manifest: &Manifest,
) -> Result<Response, AppError> {
    let body = stream_range(
        state,
        id.to_owned(),
        manifest.clone(),
        0..manifest.total_size,
    );
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, mime_header(&manifest.mime));
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(manifest.total_size));
    Ok((StatusCode::OK, h, Body::from_stream(body)).into_response())
}

fn mime_header(mime: &str) -> HeaderValue {
    HeaderValue::from_str(mime).unwrap_or(HeaderValue::from_static("application/octet-stream"))
}

fn stream_range(
    state: AppState,
    file_id: String,
    manifest: Manifest,
    range: std::ops::Range<u64>,
) -> impl futures::Stream<Item = Result<bytes::Bytes, std::io::Error>> + Send + 'static {
    let per = manifest.payload_per_monke as u64;
    let total = manifest.total_size;
    let end = range.end.min(total);
    let first = range.start / per;
    let last = if end == 0 { 0 } else { (end - 1) / per };

    let indices: Vec<u32> = (first as u32..=last as u32).collect();
    let store = state.index.store().clone();
    let manifest_arc = std::sync::Arc::new(manifest);

    stream::iter(indices).then(move |idx| {
        let store = store.clone();
        let manifest = manifest_arc.clone();
        let file_id = file_id.clone();
        async move {
            let per = manifest.payload_per_monke as u64;
            let shard_start = idx as u64 * per;
            let shard_end = (shard_start + per).min(total);
            let slice_start = range.start.max(shard_start);
            let slice_end = end.min(shard_end);
            let mf = manifest.as_ref().clone();
            let file_id_clone = file_id.clone();
            tokio::task::spawn_blocking(move || {
                let shard_for = |i: u32| store.shard_path(&file_id_clone, i, mf.num_monkes);
                read_range(&mf, &shard_for, slice_start..slice_end)
                    .map(bytes::Bytes::from)
                    .map_err(|e| std::io::Error::other(e.to_string()))
            })
            .await
            .map_err(|e| std::io::Error::other(format!("join: {e}")))?
        }
    })
}
