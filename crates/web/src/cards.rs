use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::Utc;
use domain::Store;
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
    phase: domain::Phase,
    due_label: Option<String>,
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

pub async fn deck_page<S: Store>(
    State(store): State<S>,
    Path(deck_id): Path<i64>,
) -> Result<Response, AppError> {
    render_deck_page(&store, deck_id, None, CardDraft::default()).await
}

pub async fn create_card<S: Store>(
    State(store): State<S>,
    Path(deck_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CardForm>,
) -> Result<Response, AppError> {
    match domain::create_card(
        &store,
        crate::LEGACY_USER_ID,
        deck_id,
        &form.front,
        &form.back,
    )
    .await
    {
        Ok(_) => after_change(&store, deck_id, &headers, None, CardDraft::default()).await,
        Err(domain::Error::EmptyCardFront) => {
            after_change(
                &store,
                deck_id,
                &headers,
                Some("Card front cannot be empty."),
                CardDraft::from_form(&form, None),
            )
            .await
        }
        Err(domain::Error::EmptyCardBack) => {
            after_change(
                &store,
                deck_id,
                &headers,
                Some("Card back cannot be empty."),
                CardDraft::from_form(&form, None),
            )
            .await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn update_card<S: Store>(
    State(store): State<S>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<CardForm>,
) -> Result<Response, AppError> {
    match domain::update_card(
        &store,
        crate::LEGACY_USER_ID,
        card_id,
        &form.front,
        &form.back,
    )
    .await
    {
        Ok(card) => after_change(&store, card.deck_id, &headers, None, CardDraft::default()).await,
        Err(domain::Error::EmptyCardFront) => {
            card_error(
                &store,
                card_id,
                &headers,
                "Card front cannot be empty.",
                &form,
            )
            .await
        }
        Err(domain::Error::EmptyCardBack) => {
            card_error(
                &store,
                card_id,
                &headers,
                "Card back cannot be empty.",
                &form,
            )
            .await
        }
        Err(err) => Err(err.into()),
    }
}

pub async fn delete_card<S: Store>(
    State(store): State<S>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let deck_id = domain::delete_card(&store, crate::LEGACY_USER_ID, card_id).await?;
    after_change(&store, deck_id, &headers, None, CardDraft::default()).await
}

async fn card_error<S: Store>(
    store: &S,
    card_id: i64,
    headers: &HeaderMap,
    error: &str,
    form: &CardForm,
) -> Result<Response, AppError> {
    let card = domain::get_card(store, crate::LEGACY_USER_ID, card_id)
        .await?
        .ok_or(domain::Error::CardNotFound { card_id })?;
    after_change(
        store,
        card.deck_id,
        headers,
        Some(error),
        CardDraft::from_form(form, Some(card_id)),
    )
    .await
}

async fn after_change<S: Store>(
    store: &S,
    deck_id: i64,
    headers: &HeaderMap,
    error: Option<&str>,
    draft: CardDraft,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        render_cards(store, deck_id, error, draft).await
    } else if error.is_some() {
        render_deck_page(store, deck_id, error, draft).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}")).into_response())
    }
}

async fn render_deck_page<S: Store>(
    store: &S,
    deck_id: i64,
    error: Option<&str>,
    draft: CardDraft,
) -> Result<Response, AppError> {
    let (deck, cards) = domain::list_deck_cards(store, crate::LEGACY_USER_ID, deck_id).await?;
    Ok(Html(
        DeckPageTemplate {
            deck_id: deck.id,
            deck_name: deck.name,
            cards: card_rows(cards),
            error: error.map(str::to_string),
            draft,
        }
        .render()?,
    )
    .into_response())
}

async fn render_cards<S: Store>(
    store: &S,
    deck_id: i64,
    error: Option<&str>,
    draft: CardDraft,
) -> Result<Response, AppError> {
    let (_, cards) = domain::list_deck_cards(store, crate::LEGACY_USER_ID, deck_id).await?;
    Ok(Html(
        CardsTemplate {
            deck_id,
            cards: card_rows(cards),
            error: error.map(str::to_string),
            draft,
        }
        .render()?,
    )
    .into_response())
}

fn card_rows(cards: Vec<domain::CardText>) -> Vec<CardRow> {
    let now = Utc::now();
    cards
        .into_iter()
        .map(|card| {
            let due_label = card.due_label(now);
            CardRow {
                id: card.id,
                front: card.front,
                back: card.back,
                phase: card.phase,
                due_label,
            }
        })
        .collect()
}
