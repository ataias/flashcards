use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Local;
use db::SqlitePool;
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

pub async fn home(State(pool): State<SqlitePool>) -> Result<Response, AppError> {
    render_home(&pool, None).await
}

pub async fn create_deck(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match db::create_deck(&pool, &form.name).await {
        Ok(_) => after_change(&pool, &headers, None).await,
        Err(db::Error::EmptyDeckName) => {
            after_change(&pool, &headers, Some("Deck name cannot be empty.")).await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn rename_deck(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<DeckNameForm>,
) -> Result<Response, AppError> {
    match db::rename_deck(&pool, deck_id, &form.name).await {
        Ok(_) => after_change(&pool, &headers, None).await,
        Err(db::Error::EmptyDeckName) => {
            after_change(&pool, &headers, Some("Deck name cannot be empty.")).await
        }
        Err(db::Error::DeckNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

pub async fn delete_deck(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    db::delete_deck(&pool, deck_id).await?;
    after_change(&pool, &headers, None).await
}

async fn after_change(
    pool: &SqlitePool,
    headers: &HeaderMap,
    error: Option<&str>,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_decks(pool, error).await
    } else if error.is_some() {
        render_home(pool, error).await
    } else {
        Ok(Redirect::to("/").into_response())
    }
}

async fn render_home(pool: &SqlitePool, error: Option<&str>) -> Result<Response, AppError> {
    let decks = load_rows(pool).await?;
    Ok(Html(
        HomeTemplate {
            decks,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn render_decks(pool: &SqlitePool, error: Option<&str>) -> Result<Response, AppError> {
    let decks = load_rows(pool).await?;
    Ok(Html(
        DecksTemplate {
            decks,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn load_rows(pool: &SqlitePool) -> Result<Vec<DeckRow>, AppError> {
    let summaries = db::list_deck_summaries(pool, Local::now()).await?;
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
