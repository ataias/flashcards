use askama::Template;
use axum::extract::{Form, Path, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use chrono::{Local, Utc};
use domain::{Deck, Rating, Store};
use serde::Deserialize;

use crate::error::AppError;
use crate::wants_fragment;

#[derive(Template)]
#[template(path = "study.html")]
struct StudyPageTemplate {
    deck_id: i64,
    deck_name: String,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<String>,
}

#[derive(Template)]
#[template(path = "review.html")]
struct ReviewTemplate {
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<String>,
}

struct StudyCard {
    id: i64,
    front: String,
    back: String,
}

#[derive(Deserialize)]
pub struct RateForm {
    rating: i64,
}

pub async fn study_page<S: Store>(
    State(store): State<S>,
    Path(deck_id): Path<i64>,
) -> Result<Response, AppError> {
    let deck = domain::get_deck(&store, deck_id)
        .await?
        .ok_or(domain::Error::DeckNotFound { deck_id })?;
    let card = next_card(&store, deck_id).await?;
    render_full(&deck, card, false, None)
}

pub async fn reveal<S: Store>(
    State(store): State<S>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let card = domain::get_card(&store, card_id)
        .await?
        .ok_or(domain::Error::CardNotFound { card_id })?;
    render_review(
        &store,
        card.deck_id,
        Some(study_card(&card)),
        true,
        None,
        &headers,
    )
    .await
}

pub async fn rate<S: Store>(
    State(store): State<S>,
    Path(card_id): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<RateForm>,
) -> Result<Response, AppError> {
    let Some(rating) = Rating::from_grade(form.rating) else {
        let card = domain::get_card(&store, card_id)
            .await?
            .ok_or(domain::Error::CardNotFound { card_id })?;
        return render_review(
            &store,
            card.deck_id,
            Some(study_card(&card)),
            true,
            Some("Choose Again, Hard, Good, or Easy."),
            &headers,
        )
        .await;
    };
    let (card, _) = domain::rate(&store, card_id, rating, Utc::now()).await?;
    after_rate(&store, card.deck_id, &headers).await
}

async fn after_rate<S: Store>(
    store: &S,
    deck_id: i64,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    if wants_fragment(headers) {
        let card = next_card(store, deck_id).await?;
        render_review(store, deck_id, card, false, None, headers).await
    } else {
        Ok(Redirect::to(&format!("/decks/{deck_id}/study")).into_response())
    }
}

async fn render_review<S: Store>(
    store: &S,
    deck_id: i64,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<&str>,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let deck = domain::get_deck(store, deck_id)
        .await?
        .ok_or(domain::Error::DeckNotFound { deck_id })?;
    if wants_fragment(headers) {
        Ok(Html(
            ReviewTemplate {
                card,
                revealed,
                error: error.map(str::to_string),
            }
            .render()?,
        )
        .into_response())
    } else {
        render_full(&deck, card, revealed, error)
    }
}

fn render_full(
    deck: &Deck,
    card: Option<StudyCard>,
    revealed: bool,
    error: Option<&str>,
) -> Result<Response, AppError> {
    Ok(Html(
        StudyPageTemplate {
            deck_id: deck.id,
            deck_name: deck.name.clone(),
            card,
            revealed,
            error: error.map(str::to_string),
        }
        .render()?,
    )
    .into_response())
}

async fn next_card<S: Store>(store: &S, deck_id: i64) -> Result<Option<StudyCard>, AppError> {
    Ok(domain::next_study_card(store, deck_id, Local::now())
        .await?
        .map(|card| study_card(&card)))
}

fn study_card(card: &domain::Card) -> StudyCard {
    StudyCard {
        id: card.id,
        front: card.front.clone(),
        back: card.back.clone(),
    }
}
