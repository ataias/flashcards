use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Local;
use domain::Store;
use serde::Deserialize;

use crate::error::AppError;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    decks: Vec<DeckRow>,
    error: Option<String>,
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

pub async fn home<S: Store>(State(store): State<S>) -> Result<Response, AppError> {
    render_home(&store, None).await
}

pub async fn create_deck<S: Store>(
    State(store): State<S>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match domain::create_deck(&store, &form.name).await {
        Ok(_) => after_change(&store, &headers, None).await,
        Err(domain::Error::EmptyDeckName) => {
            after_change(&store, &headers, Some("Deck name cannot be empty.")).await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn rename_deck<S: Store>(
    State(store): State<S>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match domain::rename_deck(&store, deck_id, &form.name).await {
        Ok(_) => after_change(&store, &headers, None).await,
        Err(domain::Error::EmptyDeckName) => {
            after_change(&store, &headers, Some("Deck name cannot be empty.")).await
        }
        Err(domain::Error::DeckNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

pub async fn delete_deck<S: Store>(
    State(store): State<S>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    match domain::delete_deck(&store, deck_id).await {
        Ok(()) => after_change(&store, &headers, None).await,
        Err(domain::Error::DeckNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

async fn after_change<S: Store>(
    store: &S,
    headers: &HeaderMap,
    error: Option<&str>,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_decks(store, error).await
    } else if error.is_some() {
        render_home(store, error).await
    } else {
        Ok(Redirect::to("/").into_response())
    }
}

async fn render_home<S: Store>(store: &S, error: Option<&str>) -> Result<Response, AppError> {
    let decks = load_rows(store).await?;
    Ok(Html(
        HomeTemplate {
            decks,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn render_decks<S: Store>(store: &S, error: Option<&str>) -> Result<Response, AppError> {
    let decks = load_rows(store).await?;
    Ok(Html(
        DecksTemplate {
            decks,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn load_rows<S: Store>(store: &S) -> Result<Vec<DeckRow>, AppError> {
    let summaries = domain::list_home(store, Local::now()).await?;
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
