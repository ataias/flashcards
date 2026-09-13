mod about;
mod build_info;
mod cards;
mod config;
mod decks;
mod error;
mod study;

pub use config::{Config, ConfigError, DEFAULT_BIND, DEFAULT_DB_PATH};

use axum::Router;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use domain::Store;
use tower_http::services::ServeDir;

/// Placeholder owner until HTTP sessions identify the current User.
/// `SqliteStore` ignores it until decks persist an owner.
pub(crate) const LEGACY_USER_ID: domain::UserId = 1;

fn wants_fragment(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == "true")
}

fn static_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/static")
}

pub fn app<S>(store: S) -> Router
where
    S: Store + Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(decks::home::<S>))
        .route("/about", get(about::about))
        .route("/decks", post(decks::create_deck::<S>))
        .route("/decks/{id}", get(cards::deck_page::<S>))
        .route("/decks/{id}/rename", post(decks::rename_deck::<S>))
        .route("/decks/{id}/delete", post(decks::delete_deck::<S>))
        .route("/decks/{id}/cards", post(cards::create_card::<S>))
        .route("/decks/{id}/study", get(study::study_page::<S>))
        .route("/cards/{id}", post(cards::update_card::<S>))
        .route("/cards/{id}/delete", post(cards::delete_card::<S>))
        .route("/cards/{id}/reveal", post(study::reveal::<S>))
        .route("/cards/{id}/rate", post(study::rate::<S>))
        .nest_service("/static", ServeDir::new(static_dir()))
        .with_state(store)
}

#[cfg(test)]
mod tests {
    use crate::error::AppError;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    async fn body_of(error: AppError) -> (StatusCode, String) {
        let response = error.into_response();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn not_found_is_404_without_ids() {
        let (status, body) = body_of(AppError::Domain(domain::Error::CardNotFound {
            card_id: 42,
        }))
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!body.contains("42"));
        assert!(!body.contains("card"));

        let (status, body) =
            body_of(AppError::Domain(domain::Error::DeckNotFound { deck_id: 7 })).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(!body.contains('7'));
        assert!(!body.contains("deck"));
    }

    #[tokio::test]
    async fn empty_fields_are_bad_request() {
        let (status, body) = body_of(AppError::Domain(domain::Error::EmptyDeckName)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body, "Bad request");
    }

    #[tokio::test]
    async fn storage_hides_internal_details() {
        let (status, body) = body_of(AppError::Domain(domain::Error::storage(
            std::io::Error::other("secret-path"),
        )))
        .await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body, "Internal server error");
        assert!(!body.contains("secret"));
    }
}
