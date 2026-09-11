mod config;

pub use config::{Config, ConfigError, DEFAULT_BIND, DEFAULT_DB_PATH};

use axum::response::Html;
use axum::routing::get;
use axum::Router;
use tower_http::services::ServeDir;

const INDEX_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Flashcards</title>
  <link rel="stylesheet" href="/static/app.css">
  <script src="/static/htmx.min.js" defer></script>
</head>
<body>
  <main>
    <h1>Flashcards</h1>
    <p>Placeholder server — vendored HTMX, no CDN.</p>
    <button hx-get="/ping" hx-target="#htmx-check">Check HTMX</button>
    <p id="htmx-check"></p>
  </main>
</body>
</html>
"##;

fn static_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/static")
}

pub fn app() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/ping", get(ping))
        .nest_service("/static", ServeDir::new(static_dir()))
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn ping() -> &'static str {
    "htmx ok"
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get(path: &str) -> (StatusCode, Vec<u8>) {
        let response = app()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, bytes.to_vec())
    }

    #[tokio::test]
    async fn index_references_local_static_only() {
        let (status, body) = get("/").await;
        let html = String::from_utf8(body).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("/static/htmx.min.js"));
        assert!(html.contains("/static/app.css"));
        assert!(!html.contains("cdn.jsdelivr"));
        assert!(!html.contains("unpkg.com"));
        assert!(!html.contains("cdnjs"));
    }

    #[tokio::test]
    async fn serves_vendored_htmx() {
        let (status, body) = get("/static/htmx.min.js").await;
        let js = String::from_utf8_lossy(&body);
        assert_eq!(status, StatusCode::OK);
        assert!(
            body.len() > 10_000,
            "expected real minified HTMX, got {} bytes",
            body.len()
        );
        assert!(js.contains("htmx"));
        assert!(!js.contains("cdn.jsdelivr"));
    }

    #[tokio::test]
    async fn serves_local_css() {
        let (status, body) = get("/static/app.css").await;
        let css = String::from_utf8(body).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert!(css.contains("body"));
    }
}
