mod config;
mod decks;

pub use config::{Config, ConfigError, DEFAULT_BIND, DEFAULT_DB_PATH};

use axum::Router;
use axum::routing::{get, post};
use db::SqlitePool;
use tower_http::services::ServeDir;

fn static_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/static")
}

pub fn app(pool: SqlitePool) -> Router {
    Router::new()
        .route("/", get(decks::home))
        .route("/decks", post(decks::create_deck))
        .route("/decks/{id}/rename", post(decks::rename_deck))
        .route("/decks/{id}/delete", post(decks::delete_deck))
        .nest_service("/static", ServeDir::new(static_dir()))
        .with_state(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn test_pool() -> SqlitePool {
        let path = std::env::temp_dir().join(format!(
            "flashcards-web-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        db::open(&path).await.unwrap()
    }

    async fn request(app: Router, req: Request<Body>) -> (StatusCode, String) {
        let response = app.oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes).unwrap())
    }

    async fn get(app: Router, path: &str) -> (StatusCode, String) {
        request(
            app,
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await
    }

    async fn post_form(app: Router, path: &str, body: &str, htmx: bool) -> (StatusCode, String) {
        let mut builder = Request::builder()
            .method("POST")
            .uri(path)
            .header("content-type", "application/x-www-form-urlencoded");
        if htmx {
            builder = builder.header("HX-Request", "true");
        }
        request(app, builder.body(Body::from(body.to_string())).unwrap()).await
    }

    #[tokio::test]
    async fn index_lists_default_deck_and_local_static() {
        let (status, html) = get(app(test_pool().await), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Default"));
        assert!(html.contains("due 0"));
        assert!(html.contains("new 0"));
        assert!(html.contains("hx-confirm"));
        assert!(html.contains("/static/htmx.min.js"));
        assert!(html.contains("/static/app.css"));
        assert!(!html.contains("cdn.jsdelivr"));
        assert!(!html.contains("unpkg.com"));
        assert!(!html.contains("cdnjs"));
    }

    #[tokio::test]
    async fn serves_vendored_htmx() {
        let (status, js) = get(app(test_pool().await), "/static/htmx.min.js").await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            js.len() > 10_000,
            "expected real minified HTMX, got {} bytes",
            js.len()
        );
        assert!(js.contains("htmx"));
        assert!(!js.contains("cdn.jsdelivr"));
    }

    #[tokio::test]
    async fn serves_local_css() {
        let (status, css) = get(app(test_pool().await), "/static/app.css").await;
        assert_eq!(status, StatusCode::OK);
        assert!(css.contains("body"));
    }

    #[tokio::test]
    async fn htmx_create_rename_delete_and_default_recreate() {
        let pool = test_pool().await;
        let (status, html) = post_form(app(pool.clone()), "/decks", "name=Spanish", true).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Spanish"));
        assert!(html.contains("Default"));
        assert!(html.contains("id=\"decks\""));

        let decks = db::list_decks(&pool).await.unwrap();
        let spanish = decks.iter().find(|d| d.name == "Spanish").unwrap();
        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/decks/{}/rename", spanish.id),
            "name=Italiano",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Italiano"));
        assert!(!html.contains("Spanish"));

        let default_id = db::list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == db::DEFAULT_DECK_NAME)
            .unwrap()
            .id;
        let italiano_id = db::list_decks(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == "Italiano")
            .unwrap()
            .id;

        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/decks/{default_id}/delete"),
            "",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Italiano"));
        assert!(!html.contains("Default"));

        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/decks/{italiano_id}/delete"),
            "",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Default"));
        let decks = db::list_decks(&pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, db::DEFAULT_DECK_NAME);
        assert_ne!(decks[0].id, italiano_id);
        assert_ne!(decks[0].id, default_id);
    }

    #[tokio::test]
    async fn create_rejects_empty_name() {
        let (status, html) = post_form(app(test_pool().await), "/decks", "name=+++", true).await;
        // "+" is form-urlencoded space; three pluses → whitespace-only.
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Deck name cannot be empty"));
        assert!(html.contains("Default"));
    }

    #[tokio::test]
    async fn non_htmx_create_redirects_home() {
        let pool = test_pool().await;
        let (status, _) = post_form(app(pool.clone()), "/decks", "name=French", false).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let (status, html) = get(app(pool), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("French"));
    }
}
