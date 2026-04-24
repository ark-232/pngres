use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Core(#[from] pngres_core::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("file not found: {0}")]
    NotFound(String),

    #[error("payload too large")]
    PayloadTooLarge,

    #[error("invalid range: {0}")]
    InvalidRange(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: ErrorPayload<'a>,
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    code: &'static str,
    message: String,
    hint: &'a str,
}

impl AppError {
    fn status_and_hint(&self) -> (StatusCode, &'static str, &'static str) {
        match self {
            AppError::Core(pngres_core::Error::MissingShard { .. }) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "missing_shard",
                "a monke has wandered off; check the data dir",
            ),
            AppError::Core(pngres_core::Error::HashMismatch { .. }) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "hash_mismatch",
                "a monke forgot its own face; refusing to lie to you",
            ),
            AppError::Core(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "core_error",
                "a monke has fainted. offer it a banana",
            ),
            AppError::NotFound(_) => {
                (StatusCode::NOT_FOUND, "not_found", "no monke by that name")
            }
            AppError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "that file is too big for this troop; recruit a bigger server",
            ),
            AppError::InvalidRange(_) => (
                StatusCode::RANGE_NOT_SATISFIABLE,
                "bad_range",
                "you asked for a byte range the monkes do not have; they are offended",
            ),
            AppError::BadRequest(_) => (
                StatusCode::BAD_REQUEST,
                "bad_request",
                "the monke does not understand your request",
            ),
            AppError::Io(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "io_error",
                "disk is sulking; check the herd",
            ),
            AppError::Json(_) | AppError::Other(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "something deep in the canopy broke. check the server logs",
            ),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, hint) = self.status_and_hint();
        let body = ErrorBody {
            error: ErrorPayload {
                code,
                message: self.to_string(),
                hint,
            },
        };
        (status, axum::Json(body)).into_response()
    }
}
