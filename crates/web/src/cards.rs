use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use db::SqlitePool;
use serde::Deserialize;

use crate::error::AppError;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "deck.html")]
struct DeckPageTemplate {
    deck_id: i64,
    deck_name: String,
    cards: Vec<CardRow>,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "cards.html")]
struct CardsTemplate {
    deck_id: i64,
    cards: Vec<CardRow>,
    error: Option<String>,
}

struct CardRow {
    id: i64,
    front: String,
    back: String,
}

#[derive(Deserialize)]
pub struct CardForm {
    front: String,
    back: String,
}

pub async fn deck_page(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
) -> Result<Response, AppError> {
    render_deck_page(&pool, deck_id, None).await
}

pub async fn create_card(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CardForm>,
) -> Result<Response, AppError> {
    match db::create_card(&pool, deck_id, &form.front, &form.back).await {
        Ok(_) => after_change(&pool, deck_id, &headers, None).await,
        Err(db::Error::EmptyCardFront) => {
            after_change(
                &pool,
                deck_id,
                &headers,
                Some("Card front cannot be empty."),
            )
            .await
        }
        Err(db::Error::EmptyCardBack) => {
            after_change(&pool, deck_id, &headers, Some("Card back cannot be empty.")).await
        }
        Err(db::Error::DeckNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

pub async fn update_card(
    State(pool): State<SqlitePool>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CardForm>,
) -> Result<Response, AppError> {
    match db::update_card(&pool, card_id, &form.front, &form.back).await {
        Ok(card) => after_change(&pool, card.deck_id, &headers, None).await,
        Err(db::Error::EmptyCardFront) => {
            card_error(&pool, card_id, &headers, "Card front cannot be empty.").await
        }
        Err(db::Error::EmptyCardBack) => {
            card_error(&pool, card_id, &headers, "Card back cannot be empty.").await
        }
        Err(db::Error::CardNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

pub async fn delete_card(
    State(pool): State<SqlitePool>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let Some(card) = db::get_card(&pool, card_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    db::delete_card(&pool, card_id).await?;
    after_change(&pool, card.deck_id, &headers, None).await
}

async fn card_error(
    pool: &SqlitePool,
    card_id: i64,
    headers: &HeaderMap,
    error: &str,
) -> Result<Response, AppError> {
    let Some(card) = db::get_card(pool, card_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    after_change(pool, card.deck_id, headers, Some(error)).await
}

async fn after_change(
    pool: &SqlitePool,
    deck_id: i64,
    headers: &HeaderMap,
    error: Option<&str>,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_cards(pool, deck_id, error).await
    } else if error.is_some() {
        render_deck_page(pool, deck_id, error).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
    }
}

async fn render_deck_page(
    pool: &SqlitePool,
    deck_id: i64,
    error: Option<&str>,
) -> Result<Response, AppError> {
    let Some(deck) = db::get_deck(pool, deck_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let cards = load_rows(pool, deck_id).await?;
    Ok(Html(
        DeckPageTemplate {
            deck_id: deck.id,
            deck_name: deck.name,
            cards,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn render_cards(
    pool: &SqlitePool,
    deck_id: i64,
    error: Option<&str>,
) -> Result<Response, AppError> {
    if db::get_deck(pool, deck_id).await?.is_none() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let cards = load_rows(pool, deck_id).await?;
    Ok(Html(
        CardsTemplate {
            deck_id,
            cards,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn load_rows(pool: &SqlitePool, deck_id: i64) -> Result<Vec<CardRow>, AppError> {
    let cards = db::list_cards_in_deck(pool, deck_id).await?;
    Ok(cards
        .into_iter()
        .map(|card| CardRow {
            id: card.id,
            front: card.front,
            back: card.back,
        })
        .collect())
}
