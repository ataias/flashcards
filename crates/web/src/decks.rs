use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Local;
use domain::Store;
use serde::Deserialize;

use crate::AppState;
use crate::assets::Head;
use crate::error::AppError;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    decks: Vec<DeckRow>,
    error: Option<String>,
    head: Head,
}

#[derive(Template)]
#[template(path = "decks.html")]
struct DecksTemplate {
    decks: Vec<DeckRow>,
    error: Option<String>,
}

struct DeckRow {
    id: i64,
    name: String,
    due_count: usize,
    new_count: usize,
}

#[derive(Deserialize)]
pub struct DeckNameForm {
    name: String,
}

pub async fn home<S: Store>(
    State(AppState { store, user_id }): State<AppState<S>>,
) -> Result<Response, AppError> {
    render_home(&store, user_id, None).await
}

pub async fn create_deck<S: Store>(
    State(AppState { store, user_id }): State<AppState<S>>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match domain::create_deck(&store, user_id, &form.name).await {
        Ok(_) => after_change(&store, user_id, &headers, None).await,
        Err(domain::Error::EmptyDeckName) => {
            after_change(
                &store,
                user_id,
                &headers,
                Some("Deck name cannot be empty."),
            )
            .await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn rename_deck<S: Store>(
    State(AppState { store, user_id }): State<AppState<S>>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match domain::rename_deck(&store, user_id, deck_id, &form.name).await {
        Ok(_) => after_change(&store, user_id, &headers, None).await,
        Err(domain::Error::EmptyDeckName) => {
            after_change(
                &store,
                user_id,
                &headers,
                Some("Deck name cannot be empty."),
            )
            .await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn delete_deck<S: Store>(
    State(AppState { store, user_id }): State<AppState<S>>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    domain::delete_deck(&store, user_id, deck_id).await?;
    after_change(&store, user_id, &headers, None).await
}

async fn after_change<S: Store>(
    store: &S,
    user_id: domain::UserId,
    headers: &HeaderMap,
    error: Option<&str>,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_decks(store, user_id, error).await
    } else if error.is_some() {
        render_home(store, user_id, error).await
    } else {
        Ok(Redirect::to("/").into_response())
    }
}

async fn render_home<S: Store>(
    store: &S,
    user_id: domain::UserId,
    error: Option<&str>,
) -> Result<Response, AppError> {
    let decks = load_rows(store, user_id).await?;
    Ok(Html(
        HomeTemplate {
            decks,
            error: error.map(str::to_string),
            head: Head::new("Flashcards")?,
        }
        .render()?,
    )
    .into_response())
}

async fn render_decks<S: Store>(
    store: &S,
    user_id: domain::UserId,
    error: Option<&str>,
) -> Result<Response, AppError> {
    let decks = load_rows(store, user_id).await?;
    Ok(Html(
        DecksTemplate {
            decks,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn load_rows<S: Store>(store: &S, user_id: domain::UserId) -> Result<Vec<DeckRow>, AppError> {
    let summaries = domain::list_home(store, user_id, Local::now()).await?;
    Ok(summaries
        .into_iter()
        .map(|summary| DeckRow {
            id: summary.deck.id,
            name: summary.deck.name,
            due_count: summary.due_count,
            new_count: summary.new_count,
        })
        .collect())
}
