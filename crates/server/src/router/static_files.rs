//! Embedded SPA static file serving.
//!
//! The frontend is bundled into the server binary via `rust-embed` at compile
//! time. This handler serves requested paths from the embedded bundle and falls
//! back to `index.html` for unknown paths so TanStack Router can handle
//! client-side routing.
use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../apps/web/dist"]
struct Assets;

pub async fn spa_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if let Some(file) = Assets::get(path) {
        return embedded_file_response(path, &file);
    }
    match Assets::get("index.html") {
        Some(file) => embedded_file_response("index.html", &file),
        None => (StatusCode::NOT_FOUND, "Frontend not embedded").into_response(),
    }
}

fn embedded_file_response(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut builder = Response::builder().header(header::CONTENT_TYPE, mime.as_ref());
    if path.starts_with("assets/") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=60");
    }
    builder
        .body(Body::from(file.data.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
