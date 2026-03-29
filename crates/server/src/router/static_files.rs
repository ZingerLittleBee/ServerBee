use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../apps/web/dist"]
struct Assets;

/// Serve embedded static files or fall back to index.html for SPA routing.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first
    if let Some(file) = Assets::get(path) {
        return serve_file(path, &file);
    }

    // Fallback to index.html for SPA client-side routing
    match Assets::get("index.html") {
        Some(file) => serve_file("index.html", &file),
        None => (StatusCode::NOT_FOUND, "Frontend not embedded").into_response(),
    }
}

fn serve_file(path: &str, file: &rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();

    let mut builder = Response::builder().header(header::CONTENT_TYPE, mime.as_ref());

    // Cache immutable hashed assets aggressively, everything else briefly
    if path.starts_with("assets/") {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=31536000, immutable");
    } else {
        builder = builder.header(header::CACHE_CONTROL, "public, max-age=60");
    }

    builder
        .body(Body::from(file.data.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
