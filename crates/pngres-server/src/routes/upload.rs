use axum::extract::{Multipart, State};
use axum::Json;
use serde::Serialize;

use pngres_core::{encode_bytes, EncodeOptions};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Serialize)]
pub struct UploadResponse {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub num_monkes: u32,
}

pub async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, AppError> {
    let mut filename: Option<String> = None;
    let mut mime: Option<String> = None;
    let mut data: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart parse: {e}")))?
    {
        if field.name() == Some("file") {
            filename = field.file_name().map(|s| s.to_owned());
            mime = field.content_type().map(|s| s.to_owned());
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("read field bytes: {e}")))?;
            if bytes.len() > state.max_upload_bytes {
                return Err(AppError::PayloadTooLarge);
            }
            data = Some(bytes.to_vec());
            break;
        }
    }

    let data = data.ok_or_else(|| AppError::BadRequest("no `file` field".into()))?;
    let filename = filename.unwrap_or_else(|| "unnamed.bin".to_owned());

    let effective_mime = match mime.as_deref() {
        Some(m) if m != "application/octet-stream" && !m.is_empty() => Some(m.to_owned()),
        _ => infer::get(&data).map(|k| k.mime_type().to_owned()),
    };

    let cover = state.cover.clone();
    let opts = EncodeOptions {
        filename: filename.clone(),
        mime: effective_mime,
        ..Default::default()
    };
    let (manifest, shards) =
        tokio::task::spawn_blocking(move || encode_bytes(&data, &cover, opts))
            .await
            .map_err(|e| AppError::Other(format!("join: {e}")))??;

    let id = manifest.file_id.to_string();
    let store = state.index.store();
    store.ensure_dir(&id).await?;
    for shard in &shards {
        store.write_shard(&id, shard, manifest.num_monkes).await?;
    }
    state.index.insert(manifest.clone()).await;

    Ok(Json(UploadResponse {
        id,
        name: manifest.filename,
        mime: manifest.mime,
        size: manifest.total_size,
        num_monkes: manifest.num_monkes,
    }))
}
