use axum::Router;
use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;

#[cfg(has_spa_build)]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../frontend/build"]
struct SpaAssets;

fn embedded_get(path: &str) -> Option<rust_embed::EmbeddedFile> {
    #[cfg(has_spa_build)]
    {
        SpaAssets::get(path)
    }
    #[cfg(not(has_spa_build))]
    {
        let _ = path;
        None
    }
}

fn has_real_build() -> bool {
    embedded_in_binary()
}

pub(crate) fn embedded_in_binary() -> bool {
    cfg!(has_spa_build)
}

pub(crate) fn spa_router() -> Router {
    Router::new().fallback(get(serve_spa))
}

async fn serve_spa(uri: Uri) -> Response {
    if uri.path().starts_with("/api") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if !has_real_build() {
        tracing::error!(
            "SPA bind is enabled but no frontend/build was embedded at compile time — \
             run `make spa` (or `cd frontend && pnpm build`) and rebuild mike"
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "SPA not built into this binary — run `make spa` and rebuild",
        )
            .into_response();
    }

    let path = uri.path().trim_start_matches('/');
    let candidate = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = embedded_get(candidate) {
        return serve_embedded(candidate, file);
    }

    match embedded_get("index.html") {
        Some(file) => serve_embedded("index.html", file),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn serve_embedded(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref());
    match builder.body(Body::from(file.data.into_owned())) {
        Ok(response) => response,
        Err(err) => {
            tracing::error!(%err, path, "failed to build SPA asset response");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use super::spa_router;

    #[tokio::test]
    async fn spa_router_responds_to_root() -> anyhow::Result<()> {
        let app = spa_router();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty())?)
            .await?;
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::SERVICE_UNAVAILABLE,
            "expected 200 (real build) or 503 (no build embedded), got {}",
            response.status()
        );
        Ok(())
    }

    #[tokio::test]
    async fn spa_router_falls_back_for_unknown_client_route() -> anyhow::Result<()> {
        let app = spa_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/settings/some/deep/route")
                    .body(Body::empty())?,
            )
            .await?;
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::SERVICE_UNAVAILABLE,
            "expected 200 (index.html fallback) or 503 (no build embedded), got {}",
            response.status()
        );
        Ok(())
    }

    #[tokio::test]
    async fn spa_router_rejects_api_path_with_404() -> anyhow::Result<()> {
        let app = spa_router();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/nonexistent-zzz")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }
}
