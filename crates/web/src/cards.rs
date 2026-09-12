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
    draft: CardDraft,
}

#[derive(Template)]
#[template(path = "cards.html")]
struct CardsTemplate {
    deck_id: i64,
    cards: Vec<CardRow>,
    error: Option<String>,
    draft: CardDraft,
}

struct CardRow {
    id: i64,
    front: String,
    back: String,
}

impl CardRow {
    fn form_front<'a>(&'a self, draft: &'a CardDraft) -> &'a str {
        match draft.card_id {
            Some(id) if id == self.id => draft.front.as_str(),
            _ => self.front.as_str(),
        }
    }

    fn form_back<'a>(&'a self, draft: &'a CardDraft) -> &'a str {
        match draft.card_id {
            Some(id) if id == self.id => draft.back.as_str(),
            _ => self.back.as_str(),
        }
    }
}

#[derive(Default)]
struct CardDraft {
    front: String,
    back: String,
    card_id: Option<i64>,
}

impl CardDraft {
    fn from_form(form: &CardForm, card_id: Option<i64>) -> Self {
        Self {
            front: form.front.clone(),
            back: form.back.clone(),
            card_id,
        }
    }

    fn create_front(&self) -> &str {
        if self.card_id.is_none() {
            &self.front
        } else {
            ""
        }
    }

    fn create_back(&self) -> &str {
        if self.card_id.is_none() {
            &self.back
        } else {
            ""
        }
    }
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
    render_deck_page(&pool, deck_id, None, CardDraft::default()).await
}

pub async fn create_card(
    State(pool): State<SqlitePool>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CardForm>,
) -> Result<Response, AppError> {
    match db::create_card(&pool, deck_id, &form.front, &form.back).await {
        Ok(_) => after_change(&pool, deck_id, &headers, None, CardDraft::default()).await,
        Err(db::Error::EmptyCardFront) => {
            after_change(
                &pool,
                deck_id,
                &headers,
                Some("Card front cannot be empty."),
                CardDraft::from_form(&form, None),
            )
            .await
        }
        Err(db::Error::EmptyCardBack) => {
            after_change(
                &pool,
                deck_id,
                &headers,
                Some("Card back cannot be empty."),
                CardDraft::from_form(&form, None),
            )
            .await
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
        Ok(card) => after_change(&pool, card.deck_id, &headers, None, CardDraft::default()).await,
        Err(db::Error::EmptyCardFront) => {
            card_error(
                &pool,
                card_id,
                &headers,
                "Card front cannot be empty.",
                &form,
            )
            .await
        }
        Err(db::Error::EmptyCardBack) => {
            card_error(
                &pool,
                card_id,
                &headers,
                "Card back cannot be empty.",
                &form,
            )
            .await
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
    match db::delete_card(&pool, card_id).await {
        Ok(deck_id) => after_change(&pool, deck_id, &headers, None, CardDraft::default()).await,
        Err(db::Error::CardNotFound { .. }) => Ok(StatusCode::NOT_FOUND.into_response()),
        Err(err) => Err(err.into()),
    }
}

async fn card_error(
    pool: &SqlitePool,
    card_id: i64,
    headers: &HeaderMap,
    error: &str,
    form: &CardForm,
) -> Result<Response, AppError> {
    let Some(card) = db::get_card(pool, card_id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    after_change(
        pool,
        card.deck_id,
        headers,
        Some(error),
        CardDraft::from_form(form, Some(card_id)),
    )
    .await
}

async fn after_change(
    pool: &SqlitePool,
    deck_id: i64,
    headers: &HeaderMap,
    error: Option<&str>,
    draft: CardDraft,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_cards(pool, deck_id, error, draft).await
    } else if error.is_some() {
        render_deck_page(pool, deck_id, error, draft).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
    }
}

async fn render_deck_page(
    pool: &SqlitePool,
    deck_id: i64,
    error: Option<&str>,
    draft: CardDraft,
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
            draft,
        }
        .render()?,
    )
    .into_response())
}

async fn render_cards(
    pool: &SqlitePool,
    deck_id: i64,
    error: Option<&str>,
    draft: CardDraft,
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
            draft,
        }
        .render()?,
    )
    .into_response())
}

async fn load_rows(pool: &SqlitePool, deck_id: i64) -> Result<Vec<CardRow>, AppError> {
    let cards = db::list_card_text_in_deck(pool, deck_id).await?;
    Ok(cards
        .into_iter()
        .map(|card| CardRow {
            id: card.id,
            front: card.front,
            back: card.back,
        })
        .collect())
}
