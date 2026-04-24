use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};

use crate::assets::Assets;

pub async fn index() -> Response {
    match Assets::get("index.html") {
        Some(f) => Html(std::str::from_utf8(&f.data).unwrap_or("").to_owned()).into_response(),
        None => (StatusCode::NOT_FOUND, "frontend not built").into_response(),
    }
}

pub async fn static_asset(Path(path): Path<String>) -> Response {
    match Assets::get(&path) {
        Some(f) => {
            let mime = mime_guess::from_path(&path)
                .first_or_octet_stream()
                .essence_str()
                .to_owned();
            let mut resp = (StatusCode::OK, f.data.to_vec()).into_response();
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&mime).unwrap(),
            );
            resp
        }
        None => (StatusCode::NOT_FOUND, format!("asset not found: {path}")).into_response(),
    }
}
