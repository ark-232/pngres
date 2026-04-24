use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use pngres_core::read_range;

use crate::error::AppError;
use crate::index::FileSummary;
use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct ListQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub items: Vec<FileSummary>,
    pub total: i64,
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<ListResponse>, AppError> {
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let entries = state.index.list(limit, offset).await;
    let items: Vec<FileSummary> = entries.iter().map(|e| e.summary()).collect();
    Ok(Json(ListResponse {
        total: items.len() as i64,
        items,
    }))
}

#[derive(Serialize)]
pub struct FileDetail {
    #[serde(flatten)]
    pub summary: FileSummary,
    pub manifest: pngres_core::Manifest,
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<FileDetail>, AppError> {
    let e = state
        .index
        .get_by_str(&id)
        .await
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    Ok(Json(FileDetail {
        summary: e.summary(),
        manifest: e.manifest,
    }))
}

pub async fn delete_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let parsed = uuid::Uuid::parse_str(&id).map_err(|_| AppError::NotFound(id.clone()))?;
    let existed = state.index.remove(&parsed).await?;
    if !existed {
        return Err(AppError::NotFound(id));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let entry = state
        .index
        .get_by_str(&id)
        .await
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let manifest = entry.manifest.clone();
    let store = state.index.store().clone();
    let num = manifest.num_monkes;
    let file_id = id.clone();
    let name = manifest.filename.clone();
    let mime = manifest.mime.clone();

    let bytes = tokio::task::spawn_blocking(move || {
        let shard_for = |i: u32| store.shard_path(&file_id, i, num);
        read_range(&manifest, &shard_for, 0..manifest.total_size)
    })
    .await
    .map_err(|e| AppError::Other(format!("join: {e}")))??;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&mime)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    let disposition = format!("attachment; filename=\"{}\"", sanitize(&name));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).unwrap(),
    );
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c == '"' || c == '\\' || c == '\n' || c == '\r' {
                '_'
            } else {
                c
            }
        })
        .collect()
}

// ----------------------------------------------------------- P2P: monkes

#[derive(Serialize)]
pub struct MonkeRef {
    pub index: u32,
    pub url: String,
    pub filename: String,
}

#[derive(Serialize)]
pub struct MonkeIndex {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub num_monkes: u32,
    pub lsb_bits: i64,
    pub per_monke_payload_bytes: i64,
    pub file_hash: String,
    pub monkes: Vec<MonkeRef>,
}

pub async fn list_monkes(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MonkeIndex>, AppError> {
    let e = state
        .index
        .get_by_str(&id)
        .await
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let m = &e.manifest;

    let total = m.num_monkes;
    let monkes: Vec<MonkeRef> = (0..total)
        .map(|i| {
            let width = total.saturating_sub(1).to_string().len().max(3);
            let filename = format!("monke_{:0width$}.png", i + 1, width = width);
            MonkeRef {
                index: i,
                url: format!("/api/files/{}/monkes/{}", id, i),
                filename,
            }
        })
        .collect();

    Ok(Json(MonkeIndex {
        id: id.clone(),
        name: m.filename.clone(),
        mime: m.mime.clone(),
        size: m.total_size as i64,
        num_monkes: total,
        lsb_bits: m.lsb_bits.as_u8() as i64,
        per_monke_payload_bytes: m.payload_per_monke as i64,
        file_hash: pngres_core::hash_to_hex(&m.file_hash),
        monkes,
    }))
}

pub async fn get_monke(
    State(state): State<AppState>,
    Path((id, n)): Path<(String, u32)>,
) -> Result<Response, AppError> {
    let e = state
        .index
        .get_by_str(&id)
        .await
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    let total = e.manifest.num_monkes;
    if n >= total {
        return Err(AppError::NotFound(format!(
            "monke #{} of {} for {}",
            n, total, id
        )));
    }
    let path = state.index.store().shard_path(&id, n, total);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::NotFound(format!("shard file: {}", path.display())))?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    headers.insert(header::CONTENT_LENGTH, HeaderValue::from(bytes.len()));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("inline; filename=\"monke_{:03}.png\"", n + 1)).unwrap(),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}
