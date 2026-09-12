mod cards;
mod config;
mod decks;
mod error;

pub use config::{Config, ConfigError, DEFAULT_BIND, DEFAULT_DB_PATH};

use axum::Router;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use db::SqlitePool;
use tower_http::services::ServeDir;

fn wants_fragment(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == "true")
}

fn static_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/static")
}

pub fn app(pool: SqlitePool) -> Router {
    Router::new()
        .route("/", get(decks::home))
        .route("/decks", post(decks::create_deck))
        .route("/decks/{id}", get(cards::deck_page))
        .route("/decks/{id}/rename", post(decks::rename_deck))
        .route("/decks/{id}/delete", post(decks::delete_deck))
        .route("/decks/{id}/cards", post(cards::create_card))
        .route("/cards/{id}", post(cards::update_card))
        .route("/cards/{id}/delete", post(cards::delete_card))
        .nest_service("/static", ServeDir::new(static_dir()))
        .with_state(pool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;
    use tower::ServiceExt;

    struct TestDb {
        // Drop pool before TempDir so the SQLite file can be unlinked.
        _dir: tempfile::TempDir,
        pool: SqlitePool,
    }

    async fn test_db() -> TestDb {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("flashcards.db");
        TestDb {
            pool: db::open(&path).await.unwrap(),
            _dir: dir,
        }
    }

    async fn request(app: Router, req: Request<Body>) -> (StatusCode, String) {
        let response = app.oneshot(req).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
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
        let db = test_db().await;
        let (status, html) = get(app(db.pool.clone()), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Default"));
        assert!(html.contains(&format!(
            "/decks/{}",
            db::list_decks(&db.pool).await.unwrap()[0].id
        )));
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
        let db = test_db().await;
        let (status, js) = get(app(db.pool.clone()), "/static/htmx.min.js").await;
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
        let db = test_db().await;
        let (status, css) = get(app(db.pool.clone()), "/static/app.css").await;
        assert_eq!(status, StatusCode::OK);
        assert!(css.contains("body"));
    }

    #[tokio::test]
    async fn htmx_create_rename_delete_and_default_recreate() {
        let db = test_db().await;
        let pool = &db.pool;
        let (status, html) = post_form(app(pool.clone()), "/decks", "name=Spanish", true).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Spanish"));
        assert!(html.contains("Default"));
        assert!(html.contains("id=\"decks\""));

        let decks = db::list_decks(pool).await.unwrap();
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

        let default_id = db::list_decks(pool)
            .await
            .unwrap()
            .into_iter()
            .find(|d| d.name == db::DEFAULT_DECK_NAME)
            .unwrap()
            .id;
        let italiano_id = db::list_decks(pool)
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
        let decks = db::list_decks(pool).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].name, db::DEFAULT_DECK_NAME);
        assert_ne!(decks[0].id, italiano_id);
        assert_ne!(decks[0].id, default_id);
    }

    #[tokio::test]
    async fn create_rejects_empty_name() {
        let db = test_db().await;
        let (status, html) = post_form(app(db.pool.clone()), "/decks", "name=+++", true).await;
        // "+" is form-urlencoded space; three pluses → whitespace-only.
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Deck name cannot be empty"));
        assert!(html.contains("Default"));
    }

    #[tokio::test]
    async fn non_htmx_create_redirects_home() {
        let db = test_db().await;
        let (status, _) = post_form(app(db.pool.clone()), "/decks", "name=French", false).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let (status, html) = get(app(db.pool.clone()), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("French"));
    }

    #[tokio::test]
    async fn deck_page_lists_cards_empty_then_crud() {
        let db = test_db().await;
        let pool = &db.pool;
        let deck_id = db::list_decks(pool).await.unwrap()[0].id;

        let (status, html) = get(app(pool.clone()), &format!("/decks/{deck_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Default"));
        assert!(html.contains("No Cards in this Deck yet"));
        assert!(html.contains("id=\"cards\""));

        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=Capital+of+France&back=Paris",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Capital of France"));
        assert!(html.contains("Paris"));
        assert!(html.contains("id=\"cards\""));
        assert!(!html.contains("No Cards in this Deck yet"));

        let card_id = db::list_cards_in_deck(pool, deck_id).await.unwrap()[0].id;
        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/cards/{card_id}"),
            "front=Capital+of+Italy&back=Rome",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Capital of Italy"));
        assert!(html.contains("Rome"));
        assert!(!html.contains("France"));
        assert!(!html.contains("Paris"));

        let (status, html) = post_form(
            app(pool.clone()),
            &format!("/cards/{card_id}/delete"),
            "",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("No Cards in this Deck yet"));
        assert!(!html.contains("Rome"));
        assert!(
            db::list_cards_in_deck(pool, deck_id)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn create_card_rejects_empty_sides() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let (status, html) = post_form(
            app(db.pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=+++&back=Paris",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Card front cannot be empty"));
        assert!(
            db::list_cards_in_deck(&db.pool, deck_id)
                .await
                .unwrap()
                .is_empty()
        );

        let (status, html) = post_form(
            app(db.pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=Q&back=+++",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Card back cannot be empty"));
        assert!(html.contains(">Q</textarea>"));
    }

    #[tokio::test]
    async fn create_card_preserves_draft_on_empty_front() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let (status, html) = post_form(
            app(db.pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=+++&back=Paris",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Card front cannot be empty"));
        assert!(html.contains(">Paris</textarea>"));
        assert!(html.contains("class=\"create-card\""));
    }

    #[tokio::test]
    async fn update_card_rejects_empty_sides_and_preserves_draft() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
        let (status, html) = post_form(
            app(db.pool.clone()),
            &format!("/cards/{}", card.id),
            "front=+++&back=Kept+draft",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Card front cannot be empty"));
        assert!(html.contains(">Kept draft</textarea>"));
        assert!(html.contains(">Q</p>"));
        let stored = db::get_card(&db.pool, card.id).await.unwrap().unwrap();
        assert_eq!(stored.front, "Q");
        assert_eq!(stored.back, "A");

        let (status, html) = post_form(
            app(db.pool.clone()),
            &format!("/cards/{}", card.id),
            "front=Kept+front&back=+++",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("Card back cannot be empty"));
        assert!(html.contains(">Kept front</textarea>"));
    }

    #[tokio::test]
    async fn non_htmx_update_and_delete_card_redirect_to_deck() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let card = db::create_card(&db.pool, deck_id, "Q", "A").await.unwrap();
        let (status, _) = post_form(
            app(db.pool.clone()),
            &format!("/cards/{}", card.id),
            "front=Q2&back=A2",
            false,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let (status, html) = get(app(db.pool.clone()), &format!("/decks/{deck_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains(">Q2</p>"));
        assert!(html.contains(">A2</p>"));

        let (status, _) = post_form(
            app(db.pool.clone()),
            &format!("/cards/{}/delete", card.id),
            "",
            false,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let (status, html) = get(app(db.pool.clone()), &format!("/decks/{deck_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("No Cards in this Deck yet"));
    }

    #[tokio::test]
    async fn home_counts_new_card_after_create() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let (status, _) = post_form(
            app(db.pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=Q&back=A",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let (status, html) = get(app(db.pool.clone()), "/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains("due 0"));
        assert!(html.contains("new 1"));
    }

    #[tokio::test]
    async fn non_htmx_create_card_redirects_to_deck() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        let (status, _) = post_form(
            app(db.pool.clone()),
            &format!("/decks/{deck_id}/cards"),
            "front=Q&back=A",
            false,
        )
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        let (status, html) = get(app(db.pool.clone()), &format!("/decks/{deck_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(html.contains(">Q</p>"));
        assert!(html.contains(">A</p>"));
    }

    #[tokio::test]
    async fn missing_deck_or_card_is_not_found() {
        let db = test_db().await;
        let (status, _) = get(app(db.pool.clone()), "/decks/999").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = post_form(
            app(db.pool.clone()),
            "/decks/999/cards",
            "front=Q&back=A",
            true,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) =
            post_form(app(db.pool.clone()), "/cards/999", "front=Q&back=A", true).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (status, _) = post_form(app(db.pool.clone()), "/cards/999/delete", "", true).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn card_text_is_html_escaped() {
        let db = test_db().await;
        let deck_id = db::list_decks(&db.pool).await.unwrap()[0].id;
        db::create_card(&db.pool, deck_id, "<script>alert(1)</script>", "a&b")
            .await
            .unwrap();
        let (status, html) = get(app(db.pool.clone()), &format!("/decks/{deck_id}")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));
        assert!(html.contains("a&#38;b"));
    }

    #[tokio::test]
    async fn app_error_hides_internal_details() {
        let response = AppError::Db(db::Error::CardNotFound { card_id: 42 }).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = String::from_utf8(bytes.to_vec()).unwrap();
        assert_eq!(body, "Internal server error");
        assert!(!body.contains("42"));
        assert!(!body.contains("card"));
    }
}
